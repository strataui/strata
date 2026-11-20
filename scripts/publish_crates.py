#!/usr/bin/env python3
"""Automatically version-bump changed crates and idempotently publish Strata workspace crates and strata-earth to crates.io.

Workflow per workspace:
1. Inspect workspace crates via `cargo metadata` (respecting `publish = false`).
2. Detect crates with uncommitted or untracked files in git (including external path
   dependencies in sibling repos such as `earth` -> `strata`) and skip both those
   dirty crates and every crate that transitively depends on them (unless `--allow-dirty`
   is explicitly passed). Clean crates that do not depend on dirty crates are still
   bumped and published.
3. Query crates.io metadata (`https://crates.io/api/v1/crates/{name}`) and cached
   published `.crate` archives (`https://static.crates.io/crates/{name}/{name}-{ver}.crate`)
   to detect clean crates whose packageable source/manifest files changed since their last
   published version.
4. Bump patch versions (`X.Y.Z` -> `X.Y.(Z+1)`) ONLY for changed clean crates whose current
   local version is already published on crates.io, updating each crate's `Cargo.toml`,
   root `[workspace.dependencies]`, any internal explicit version requirements, and
   `Cargo.lock` (unless `--no-bump` or `--dry-run` is passed; `--bump-only` performs
   these bumps and exits cleanly).
5. Publish unpublished clean crate versions in topological dependency order, prioritizing
   updates to already-published crates before brand-new (`404`) crates, and automatically
   waiting out HTTP 429 rate limits using crates.io's reset timestamp (`Please try again after ...`).

Requires `cargo login` or `CARGO_REGISTRY_TOKEN`. Unknown CLI flags are forwarded directly
to `cargo publish` (e.g. `--dry-run`, `--allow-dirty`, `-j 4`).
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import datetime as dt
import email.utils
import glob
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

USER_AGENT = "strata-publish/2.1"
RETRY_AFTER_DATE_RE = re.compile(
    r"Please try again after\s+([A-Za-z]{3},\s+\d{1,2}\s+[A-Za-z]{3}\s+\d{4}\s+\d{2}:\d{2}:\d{2}\s+(?:GMT|UTC|[+-]\d{4}))",
    re.IGNORECASE,
)
RETRY_AFTER_ISO_RE = re.compile(
    r"Please try again after\s+(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)",
    re.IGNORECASE,
)
RETRY_AFTER_SECS_RE = re.compile(
    r"(?:retry-after\s*[:=]\s*|try again in\s+)(\d+)\s*(?:s|sec|secs|seconds?)?\b",
    re.IGNORECASE,
)
DELETED_CRATE_REUSE_RE = re.compile(
    r"A crate with the name\s+[`'\"]?([A-Za-z0-9_-]+)[`'\"]?\s+was recently deleted\.\s*"
    r"Reuse of this name will be available after\s+"
    r"(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)",
    re.IGNORECASE,
)
FAILED_PUBLISH_CRATE_RE = re.compile(
    r"failed to publish\s+([A-Za-z0-9_-]+)\s+v",
    re.IGNORECASE,
)
CLOCK_SKEW_BUFFER_SECS = 3.0
FALLBACK_RATE_LIMIT_SECS = 600.0
CARGO_GENERATED_FILES = frozenset(
    {
        "Cargo.toml",
        "Cargo.toml.orig",
        ".cargo_vcs_info.json",
        "Cargo.lock",
    }
)


@dataclass(frozen=True)
class WorkspaceCrate:
    """Metadata for one publishable workspace crate."""

    name: str
    version: str
    manifest_path: Path
    pkg_dir: Path
    workspace_deps: tuple[str, ...]


@dataclass(frozen=True)
class CratesIoInfo:
    """Registry snapshot for a crate on crates.io."""

    exists: bool
    is_strataui_repo: bool
    active_versions: frozenset[str]
    all_versions: frozenset[str]
    latest_version: str | None


@dataclass(frozen=True)
class BumpPlan:
    """Version bump calculated for a changed published crate."""

    crate: WorkspaceCrate
    old_version: str
    new_version: str
    changed_files: tuple[str, ...]
    manifest_changed: bool


@dataclass(frozen=True)
class DirtyCrateReport:
    """Dirty crates in a workspace split into directly dirty vs transitively affected."""

    direct: frozenset[str]
    transitive: frozenset[str]
    external_causes: dict[str, tuple[str, ...]]

    @property
    def all_dirty(self) -> frozenset[str]:
        return self.direct | self.transitive


def workspace_version(workspace_toml: Path) -> str:
    """Return `[workspace.package].version` from a workspace Cargo.toml."""
    with workspace_toml.open("rb") as f:
        data = tomllib.load(f)
    ver = data.get("workspace", {}).get("package", {}).get("version")
    return ver if isinstance(ver, str) else "0.1.0"


def collect_workspace_crates(repo_root: Path) -> list[WorkspaceCrate]:
    """Return publishable workspace crates in deterministic name order via `cargo metadata`."""
    meta = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=str(repo_root),
        text=True,
        capture_output=True,
        check=False,
    )
    if meta.returncode != 0:
        raise RuntimeError(f"cargo metadata failed in {repo_root}: {meta.stderr.strip()}")

    data = json.loads(meta.stdout)
    ws_ids = set(data.get("workspace_members", []))
    all_ws_names = {
        pkg["name"]
        for pkg in data.get("packages", [])
        if pkg.get("id") in ws_ids
    }
    crates: list[WorkspaceCrate] = []
    for pkg in data.get("packages", []):
        if pkg.get("id") not in ws_ids:
            continue
        if pkg.get("publish") == []:
            continue
        name = pkg.get("name")
        ver = pkg.get("version")
        manifest = Path(pkg["manifest_path"]).resolve()
        ws_deps: list[str] = []
        for dep in pkg.get("dependencies", []):
            if dep.get("kind") == "dev":
                continue
            dep_name = dep.get("name")
            if dep.get("path") and dep_name in all_ws_names and dep_name != name:
                ws_deps.append(dep_name)
        crates.append(
            WorkspaceCrate(
                name=name,
                version=ver,
                manifest_path=manifest,
                pkg_dir=manifest.parent,
                workspace_deps=tuple(sorted(set(ws_deps))),
            )
        )
    crates.sort(key=lambda c: c.name)
    return crates


def collect_publishable_crates(repo_root: Path) -> list[tuple[str, str]]:
    """Return sorted `(crate_name, version)` pairs for publishable crates in `repo_root`."""
    try:
        return [(c.name, c.version) for c in collect_workspace_crates(repo_root)]
    except Exception:
        default_ver = workspace_version(repo_root / "Cargo.toml")
        crates: list[tuple[str, str]] = []
        for manifest_path in sorted(glob.glob(str(repo_root / "crates" / "**" / "Cargo.toml"), recursive=True)):
            with open(manifest_path, "rb") as f:
                data = tomllib.load(f)
            pkg = data.get("package", {})
            if not pkg or pkg.get("publish") is False:
                continue
            name = pkg.get("name")
            if not isinstance(name, str) or not name:
                continue
            ver = pkg.get("version")
            if isinstance(ver, dict):
                ver = default_ver
            crates.append((name, str(ver)))
        return crates


def list_dirty_repo_paths(repo_root: Path) -> set[Path]:
    """Return paths of uncommitted (staged, unstaged, or untracked) files in git."""
    res = subprocess.run(
        ["git", "-C", str(repo_root), "status", "--porcelain", "-z", "--untracked-files=all"],
        text=True,
        encoding="utf-8",
        errors="surrogateescape",
        capture_output=True,
        check=False,
    )
    if res.returncode != 0:
        if (repo_root / ".git").exists():
            raise RuntimeError(f"git status failed in {repo_root}: {res.stderr.strip()}")
        return set()
    if not res.stdout:
        return set()

    resolved_root = repo_root.resolve()
    dirty_paths: set[Path] = set()
    entries = res.stdout.split("\0")
    i = 0
    while i < len(entries):
        entry = entries[i]
        i += 1
        if len(entry) < 4:
            continue
        status = entry[:2]
        raw_path = entry[3:]
        if raw_path:
            dirty_paths.add(Path(os.path.normpath(resolved_root / raw_path)))
        if ("R" in status or "C" in status) and i < len(entries):
            # In `git status --porcelain -z`, renames and copies are followed by the original path.
            i += 1
    return dirty_paths


def _find_git_root(path: Path) -> Path | None:
    """Return the resolved git repository top-level directory containing `path`, or None."""
    target = path if path.is_dir() else path.parent
    if not target.exists():
        return None
    res = subprocess.run(
        ["git", "-C", str(target), "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=False,
    )
    if res.returncode != 0 or not res.stdout.strip():
        return None
    return Path(res.stdout.strip()).resolve()


def collect_dirty_crates(
    repo_root: Path,
    *,
    ignore_paths: set[Path] | None = None,
    ignore_paths_by_repo: dict[Path, set[Path]] | None = None,
    _repo_cache: dict[Path, DirtyCrateReport] | None = None,
    _visiting_repos: set[Path] | None = None,
) -> DirtyCrateReport:
    """Return crates with uncommitted/untracked files and all crates that transitively depend on them.

    Inspects both intra-workspace dependencies and external path dependencies in sibling git
    repositories (e.g. `earth` -> `strata`). Files outside any crate (such as `scripts/`) do not
    mark crates dirty, while an uncommitted workspace root `Cargo.toml` marks all workspace crates
    dirty.
    """
    resolved_root = repo_root.resolve()
    cache = _repo_cache if _repo_cache is not None else {}
    visiting = _visiting_repos if _visiting_repos is not None else set()
    if resolved_root in cache:
        return cache[resolved_root]
    if resolved_root in visiting:
        return DirtyCrateReport(frozenset(), frozenset(), {})
    visiting.add(resolved_root)

    try:
        effective_ignore: set[Path] = set()
        if ignore_paths:
            effective_ignore.update(Path(os.path.normpath(p.resolve())) for p in ignore_paths)
        if ignore_paths_by_repo and resolved_root in ignore_paths_by_repo:
            effective_ignore.update(
                Path(os.path.normpath(p.resolve())) for p in ignore_paths_by_repo[resolved_root]
            )

        dirty_paths = list_dirty_repo_paths(resolved_root) - effective_ignore

        meta = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=str(resolved_root),
            text=True,
            capture_output=True,
            check=False,
        )
        if meta.returncode != 0:
            if not dirty_paths:
                report = DirtyCrateReport(frozenset(), frozenset(), {})
                cache[resolved_root] = report
                return report
            # Fallback if cargo metadata is unavailable: map dirty paths to nearest package Cargo.toml.
            direct_fallback: set[str] = set()
            for p in dirty_paths:
                cur = p if p.is_dir() else p.parent
                while cur != cur.parent:
                    manifest = cur / "Cargo.toml"
                    if manifest.is_file():
                        try:
                            name = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {}).get("name")
                            if isinstance(name, str) and name:
                                direct_fallback.add(name)
                        except Exception:
                            pass
                        break
                    if cur == resolved_root:
                        break
                    cur = cur.parent
            report = DirtyCrateReport(frozenset(direct_fallback), frozenset(), {})
            cache[resolved_root] = report
            return report

        data = json.loads(meta.stdout)
        ws_ids = set(data.get("workspace_members", []))
        ws_packages = [pkg for pkg in data.get("packages", []) if pkg.get("id") in ws_ids]
        ws_names: set[str] = {str(pkg["name"]) for pkg in ws_packages if pkg.get("name")}

        pkg_dirs: list[tuple[Path, str]] = []
        extra_files_to_pkgs: dict[Path, set[str]] = {}
        rev_deps: dict[str, set[str]] = {}
        external_deps: dict[str, list[tuple[str, Path]]] = {}

        root_manifest = Path(os.path.normpath(resolved_root / "Cargo.toml"))
        root_lockfile = Path(os.path.normpath(resolved_root / "Cargo.lock"))

        for pkg in ws_packages:
            pkg_name = pkg.get("name")
            if not isinstance(pkg_name, str) or not pkg_name:
                continue
            pkg_dir = Path(os.path.normpath(Path(pkg["manifest_path"]).resolve().parent))
            pkg_dirs.append((pkg_dir, pkg_name))

            readme = pkg.get("readme")
            if isinstance(readme, str) and readme:
                readme_path = Path(os.path.normpath(pkg_dir / readme))
                extra_files_to_pkgs.setdefault(readme_path, set()).add(pkg_name)

            has_bin = False
            for target in pkg.get("targets", []):
                src_path = target.get("src_path")
                if isinstance(src_path, str) and src_path:
                    resolved_src = Path(os.path.normpath(Path(src_path).resolve()))
                    extra_files_to_pkgs.setdefault(resolved_src, set()).add(pkg_name)
                if "bin" in target.get("kind", []):
                    has_bin = True
            if has_bin:
                extra_files_to_pkgs.setdefault(root_lockfile, set()).add(pkg_name)

            for dep in pkg.get("dependencies", []):
                if dep.get("kind") == "dev":
                    continue
                dep_name = dep.get("name")
                dep_path = dep.get("path")
                if not isinstance(dep_name, str) or not dep_name or not dep_path:
                    continue
                if dep_name in ws_names:
                    if dep_name != pkg_name:
                        rev_deps.setdefault(dep_name, set()).add(pkg_name)
                else:
                    external_deps.setdefault(pkg_name, []).append(
                        (dep_name, Path(os.path.normpath(Path(dep_path).resolve())))
                    )

        # Match deepest pkg_dir first so nested packages map to the inner crate.
        pkg_dirs.sort(key=lambda item: len(item[0].parts), reverse=True)

        direct_dirty: set[str] = set()
        for p in dirty_paths:
            if p == root_manifest:
                direct_dirty.update(ws_names)
                continue
            if p in extra_files_to_pkgs:
                direct_dirty.update(extra_files_to_pkgs[p])
            for pkg_dir, pkg_name in pkg_dirs:
                if p == pkg_dir or pkg_dir in p.parents:
                    direct_dirty.add(pkg_name)
                    break

        # Check external path dependencies (e.g. earth -> strata).
        external_causes: dict[str, set[str]] = {}
        for pkg_name, deps in external_deps.items():
            for dep_name, dep_dir in deps:
                ext_git_root = _find_git_root(dep_dir)
                if ext_git_root is None or ext_git_root == resolved_root:
                    continue
                ext_report = collect_dirty_crates(
                    ext_git_root,
                    ignore_paths_by_repo=ignore_paths_by_repo,
                    _repo_cache=cache,
                    _visiting_repos=visiting,
                )
                if dep_name in ext_report.all_dirty:
                    external_causes.setdefault(pkg_name, set()).add(dep_name)

        # Propagate transitively through intra-workspace reverse dependencies.
        affected: set[str] = set(direct_dirty) | set(external_causes.keys())
        queue = list(affected)
        while queue:
            cur_pkg = queue.pop()
            for dependent in rev_deps.get(cur_pkg, ()):
                if dependent not in affected:
                    affected.add(dependent)
                    queue.append(dependent)

        transitive_only = affected - direct_dirty
        sorted_ext_causes = {k: tuple(sorted(v)) for k, v in external_causes.items()}
        report = DirtyCrateReport(
            direct=frozenset(direct_dirty),
            transitive=frozenset(transitive_only),
            external_causes=sorted_ext_causes,
        )
        cache[resolved_root] = report
        return report
    finally:
        visiting.discard(resolved_root)


def _format_name_preview(names: list[str], limit: int = 8) -> str:
    preview = ", ".join(names[:limit])
    if len(names) > limit:
        preview += f", ... (+{len(names) - limit} more)"
    return preview


def _format_dirty_crates_summary(
    label: str,
    considered_names: set[str],
    report: DirtyCrateReport,
) -> str | None:
    """Format a concise summary of skipped directly-dirty and transitively-affected crates."""
    direct = sorted(report.direct & considered_names)
    transitive = sorted(report.transitive & considered_names)
    total = len(direct) + len(transitive)
    if total == 0:
        return None
    parts: list[str] = []
    if direct:
        parts.append(f"{len(direct)} directly dirty: {_format_name_preview(direct)}")
    if transitive:
        annotated = [
            f"{name} <- {', '.join(report.external_causes[name])}"
            if name in report.external_causes
            else name
            for name in transitive
        ]
        parts.append(f"{len(transitive)} depending on dirty crates: {_format_name_preview(annotated)}")
    return (
        f"[{label}] Skipping {total} crate(s) due to uncommitted/untracked files "
        f"({'; '.join(parts)})."
    )


def sparse_index_url(name: str) -> str:
    """Return the CDN-backed crates.io sparse index URL (`https://index.crates.io/...`) for `name`."""
    low = name.lower()
    if len(low) == 1:
        rel = f"1/{low}"
    elif len(low) == 2:
        rel = f"2/{low}"
    elif len(low) == 3:
        rel = f"3/{low[0]}/{low}"
    else:
        rel = f"{low[:2]}/{low[2:4]}/{low}"
    return f"https://index.crates.io/{rel}"


def _extract_archive_repository(raw_tar_gz: bytes, name: str, version: str) -> str:
    """Extract `[package].repository` from `Cargo.toml` or `Cargo.toml.orig` inside a `.crate` archive."""
    prefix = f"{name}-{version}/"
    with tarfile.open(fileobj=io.BytesIO(raw_tar_gz), mode="r:gz") as tf:
        for candidate in (f"{prefix}Cargo.toml", f"{prefix}Cargo.toml.orig"):
            try:
                member = tf.getmember(candidate)
            except KeyError:
                continue
            f = tf.extractfile(member)
            if f is None:
                continue
            try:
                data = tomllib.loads(f.read().decode("utf-8"))
                repo = data.get("package", {}).get("repository")
                if isinstance(repo, str):
                    return repo
            except Exception:
                continue
    return ""


def fetch_crates_io_info(name: str, cache_dir: Path | None = None) -> CratesIoInfo:
    """Query the CDN-backed crates.io sparse index for `name` and verify `strataui` ownership."""
    url = sparse_index_url(name)
    last_err: Exception | None = None
    lines: list[dict[str, object]] | None = None
    for attempt in range(4):
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                raw_text = resp.read().decode("utf-8")
            lines = [json.loads(line) for line in raw_text.splitlines() if line.strip()]
            break
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return CratesIoInfo(
                    exists=False,
                    is_strataui_repo=False,
                    active_versions=frozenset(),
                    all_versions=frozenset(),
                    latest_version=None,
                )
            last_err = e
            if attempt < 3:
                time.sleep(1.0 * (attempt + 1))
                continue
            break
        except Exception as e:
            last_err = e
            if attempt < 3:
                time.sleep(1.0 * (attempt + 1))
                continue
            break

    if lines is None:
        raise RuntimeError(f"crates.io sparse index lookup for {name} failed: {last_err}")

    active_list = [str(entry["vers"]) for entry in lines if not entry.get("yanked")]
    all_list = [str(entry["vers"]) for entry in lines if "vers" in entry]
    if not all_list:
        return CratesIoInfo(
            exists=False,
            is_strataui_repo=False,
            active_versions=frozenset(),
            all_versions=frozenset(),
            latest_version=None,
        )

    latest = active_list[-1] if active_list else all_list[-1]
    effective_cache = cache_dir or Path(os.environ.get("TMPDIR", "/tmp")) / "strata_crates_io_cache"
    raw_archive = download_crate_archive(name, latest, effective_cache)
    repo = _extract_archive_repository(raw_archive, name, latest)
    return CratesIoInfo(
        exists=True,
        is_strataui_repo=("strataui" in repo),
        active_versions=frozenset(active_list),
        all_versions=frozenset(all_list),
        latest_version=active_list[-1] if active_list else None,
    )


def is_published_on_crates_io(name: str, version: str, cache_dir: Path | None = None) -> bool:
    """Return True if `name` at `version` is already published under a `strataui` repository on crates.io."""
    info = fetch_crates_io_info(name, cache_dir=cache_dir)
    return info.is_strataui_repo and version in info.active_versions


def split_published(
    crates: list[tuple[str, str]],
    cache_dir: Path | None = None,
) -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    """Partition `crates` into `(published, unpublished)` lists by querying crates.io."""
    published: list[tuple[str, str]] = []
    unpublished: list[tuple[str, str]] = []
    for name, ver in crates:
        if is_published_on_crates_io(name, ver, cache_dir=cache_dir):
            published.append((name, ver))
        else:
            unpublished.append((name, ver))
    return published, unpublished


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def download_crate_archive(name: str, version: str, cache_dir: Path) -> bytes:
    """Return cached `.crate` tarball bytes from `static.crates.io`."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    cached = cache_dir / f"{name}-{version}.crate"
    if cached.is_file():
        return cached.read_bytes()

    url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
    last_err: Exception | None = None
    for attempt in range(3):
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                raw = resp.read()
            tmp = cached.with_suffix(".crate.tmp")
            tmp.write_bytes(raw)
            tmp.replace(cached)
            return raw
        except Exception as e:
            last_err = e
            if attempt < 2:
                time.sleep(1.0 * (attempt + 1))
                continue
    raise RuntimeError(f"failed downloading {url}: {last_err}")


def extract_crate_archive(raw_tar_gz: bytes, name: str, version: str) -> tuple[dict[str, str], bytes | None]:
    """Extract `{relpath: sha256}` and `Cargo.toml.orig` bytes from a `.crate` tarball."""
    prefix = f"{name}-{version}/"
    files: dict[str, str] = {}
    cargo_orig: bytes | None = None
    with tarfile.open(fileobj=io.BytesIO(raw_tar_gz), mode="r:gz") as tf:
        for member in tf.getmembers():
            if not member.isfile():
                continue
            rel = member.name[len(prefix):] if member.name.startswith(prefix) else member.name
            if rel == "Cargo.toml.orig":
                f = tf.extractfile(member)
                cargo_orig = f.read() if f is not None else None
                continue
            if rel in CARGO_GENERATED_FILES:
                continue
            f = tf.extractfile(member)
            files[rel] = _sha256_bytes(f.read() if f is not None else b"")
    return files, cargo_orig


def load_local_package_contents(repo_root: Path, crate: WorkspaceCrate) -> tuple[dict[str, str], bytes]:
    """Return local `{relpath: sha256}` (via `cargo package --list --allow-dirty`) and `Cargo.toml` bytes."""
    res = subprocess.run(
        ["cargo", "package", "--list", "--allow-dirty", "-p", crate.name],
        cwd=str(repo_root),
        text=True,
        capture_output=True,
        check=False,
    )
    if res.returncode != 0:
        raise RuntimeError(f"cargo package --list failed for {crate.name}: {res.stderr.strip()}")

    files: dict[str, str] = {}
    for rel in res.stdout.splitlines():
        rel = rel.strip()
        if not rel or rel in CARGO_GENERATED_FILES:
            continue
        fp = crate.pkg_dir / rel
        if fp.is_file():
            files[rel] = _sha256_bytes(fp.read_bytes())
    return files, crate.manifest_path.read_bytes()


def normalize_cargo_toml_for_diff(raw_toml: bytes) -> dict[str, object]:
    """Parse `Cargo.toml` for comparison, ignoring `[package].version` representation differences."""
    data = tomllib.loads(raw_toml.decode("utf-8"))
    pkg = data.get("package")
    if isinstance(pkg, dict) and "version" in pkg:
        pkg = dict(pkg)
        pkg.pop("version", None)
        data["package"] = pkg
    return data


def has_crate_changed_since_publish(
    repo_root: Path,
    crate: WorkspaceCrate,
    cache_dir: Path,
) -> tuple[bool, tuple[str, ...], bool]:
    """Compare local crate package contents against its published `.crate` archive on crates.io.

    Returns `(changed, diff_file_paths, manifest_changed)`.
    """
    raw_archive = download_crate_archive(crate.name, crate.version, cache_dir)
    remote_files, remote_orig = extract_crate_archive(raw_archive, crate.name, crate.version)
    local_files, local_toml = load_local_package_contents(repo_root, crate)

    diff_files = tuple(
        path
        for path in sorted(set(remote_files) | set(local_files))
        if remote_files.get(path) != local_files.get(path)
    )
    manifest_changed = (
        remote_orig is not None
        and normalize_cargo_toml_for_diff(remote_orig) != normalize_cargo_toml_for_diff(local_toml)
    )
    return bool(diff_files or manifest_changed), diff_files, manifest_changed


def bump_patch_version(version: str, forbidden: frozenset[str] = frozenset()) -> str:
    """Increment the patch component of `X.Y.Z`, skipping any versions in `forbidden`."""
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(.*)", version.strip())
    if not match:
        raise ValueError(f"Unsupported non-semver package version: {version!r}")
    major, minor, patch, suffix = int(match.group(1)), int(match.group(2)), int(match.group(3)), match.group(4)
    while True:
        patch += 1
        candidate = f"{major}.{minor}.{patch}{suffix}"
        if candidate not in forbidden:
            return candidate


def _section_range(lines: list[str], section_name: str) -> tuple[int, int] | None:
    header_re = re.compile(r"^\s*\[([^\]]+)\]\s*(?:#.*)?$")
    start_idx: int | None = None
    for i, line in enumerate(lines):
        m = header_re.match(line)
        if not m:
            continue
        sec = m.group(1).strip()
        if start_idx is None:
            if sec == section_name:
                start_idx = i + 1
        else:
            return start_idx, i
    if start_idx is not None:
        return start_idx, len(lines)
    return None


def bump_manifest_version(manifest_path: Path, new_version: str) -> bool:
    """Update `[package].version` (whether literal or `.workspace = true`) to `new_version`."""
    content = manifest_path.read_text(encoding="utf-8")
    lines = content.splitlines(keepends=True)
    bounds = _section_range(lines, "package")
    if bounds is None:
        raise ValueError(f"Missing [package] section in {manifest_path}")

    start, end = bounds
    ver_line_re = re.compile(r"^(\s*)version(?:\.workspace)?\s*=.*$")
    replaced = False
    for idx in range(start, end):
        m = ver_line_re.match(lines[idx].rstrip("\r\n"))
        if m:
            eol = "\r\n" if lines[idx].endswith("\r\n") else "\n"
            lines[idx] = f'{m.group(1)}version = "{new_version}"{eol}'
            replaced = True
            break
    if not replaced:
        raise ValueError(f"Could not find `version` field in [package] of {manifest_path}")

    new_content = "".join(lines)
    parsed = tomllib.loads(new_content)
    if parsed.get("package", {}).get("version") != new_version:
        raise AssertionError(f"Version verification failed after rewriting {manifest_path}")
    if new_content != content:
        manifest_path.write_text(new_content, encoding="utf-8")
        return True
    return False


def bump_workspace_dependency_versions(
    workspace_toml: Path,
    version_by_crate_name: dict[str, str],
) -> bool:
    """Update `version = "..."` entries in root `[workspace.dependencies]` for bumped crates."""
    if not workspace_toml.is_file() or not version_by_crate_name:
        return False

    content = workspace_toml.read_text(encoding="utf-8")
    parsed = tomllib.loads(content)
    ws_deps = parsed.get("workspace", {}).get("dependencies", {})
    if not isinstance(ws_deps, dict):
        return False

    repo_root = workspace_toml.parent.resolve()
    key_to_new_ver: dict[str, str] = {}
    for dep_key, dep_val in ws_deps.items():
        if not isinstance(dep_val, dict):
            continue
        pkg_name = dep_val.get("package") or dep_key
        if pkg_name in version_by_crate_name:
            key_to_new_ver[dep_key] = version_by_crate_name[pkg_name]
        elif "path" in dep_val:
            dep_dir = (repo_root / dep_val["path"]).resolve()
            dep_manifest = dep_dir / "Cargo.toml"
            if dep_manifest.is_file():
                dep_pkg = tomllib.loads(dep_manifest.read_text(encoding="utf-8")).get("package", {}).get("name")
                if dep_pkg in version_by_crate_name:
                    key_to_new_ver[dep_key] = version_by_crate_name[dep_pkg]

    if not key_to_new_ver:
        return False

    lines = content.splitlines(keepends=True)
    bounds = _section_range(lines, "workspace.dependencies")
    if bounds is None:
        return False
    start, end = bounds
    updated_keys: set[str] = set()
    for idx in range(start, end):
        raw_line = lines[idx]
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = re.match(r"^(\s*)([A-Za-z0-9_-]+)\s*=\s*(.*)$", raw_line.rstrip("\r\n"))
        if not m:
            continue
        dep_key = m.group(2)
        if dep_key not in key_to_new_ver:
            continue
        new_ver = key_to_new_ver[dep_key]
        eol = "\r\n" if raw_line.endswith("\r\n") else "\n"
        rhs = m.group(3)
        if re.search(r'\bversion\s*=\s*"[^"]*"', rhs):
            new_rhs = re.sub(r'(\bversion\s*=\s*)"[^"]*"', rf'\1"{new_ver}"', rhs, count=1)
        elif rhs.startswith("{"):
            new_rhs = re.sub(r"^\{", f'{{ version = "{new_ver}",', rhs, count=1)
        else:
            continue
        lines[idx] = f"{m.group(1)}{dep_key} = {new_rhs}{eol}"
        updated_keys.add(dep_key)

    new_content = "".join(lines)
    reparsed = tomllib.loads(new_content)
    ws_reparsed = reparsed.get("workspace", {}).get("dependencies", {})
    for dep_key in updated_keys:
        if ws_reparsed.get(dep_key, {}).get("version") != key_to_new_ver[dep_key]:
            raise AssertionError(f"Failed verifying [workspace.dependencies].{dep_key} version update")

    if new_content != content:
        workspace_toml.write_text(new_content, encoding="utf-8")
        return True
    return False


def bump_explicit_internal_dependency_versions(
    crates: list[WorkspaceCrate],
    version_by_crate_name: dict[str, str],
) -> set[Path]:
    """Update non-workspace internal dependency specs that pin an explicit `version` alongside `path`."""
    if not version_by_crate_name:
        return set()

    pkg_dir_to_target = {
        c.pkg_dir.resolve(): (c.name, version_by_crate_name[c.name])
        for c in crates
        if c.name in version_by_crate_name
    }
    touched: set[Path] = set()
    for crate in crates:
        content = crate.manifest_path.read_text(encoding="utf-8")
        lines = content.splitlines(keepends=True)
        changed = False
        for idx, raw_line in enumerate(lines):
            if "path" not in raw_line or "version" not in raw_line or "workspace" in raw_line:
                continue
            path_match = re.search(r'\bpath\s*=\s*"([^"]+)"', raw_line)
            if not path_match:
                continue
            target_dir = (crate.pkg_dir / path_match.group(1)).resolve()
            target_info = pkg_dir_to_target.get(target_dir)
            if not target_info:
                continue
            _, new_ver = target_info
            updated = re.sub(r'(\bversion\s*=\s*)"[^"]*"', rf'\1"{new_ver}"', raw_line, count=1)
            if updated != raw_line:
                lines[idx] = updated
                changed = True
        if changed:
            new_content = "".join(lines)
            tomllib.loads(new_content)
            crate.manifest_path.write_text(new_content, encoding="utf-8")
            touched.add(crate.manifest_path.resolve())
    return touched


def auto_bump_changed_crates(
    label: str,
    repo_root: Path,
    crates: list[WorkspaceCrate],
    info_map: dict[str, CratesIoInfo],
    cache_dir: Path,
    apply: bool,
) -> tuple[list[BumpPlan], set[Path]]:
    """Detect published crates whose contents changed since their last publish and bump patch versions.

    Returns `(bump_plans, modified_paths)`.
    """
    plans: list[BumpPlan] = []
    for crate in crates:
        info = info_map[crate.name]
        if not info.is_strataui_repo or crate.version not in info.active_versions:
            continue
        changed, diff_files, manifest_changed = has_crate_changed_since_publish(repo_root, crate, cache_dir)
        if not changed:
            continue
        new_ver = bump_patch_version(crate.version, forbidden=info.all_versions)
        plans.append(
            BumpPlan(
                crate=crate,
                old_version=crate.version,
                new_version=new_ver,
                changed_files=diff_files,
                manifest_changed=manifest_changed,
            )
        )

    if not plans:
        print(f"[{label}] No published crates changed since their last crates.io release.", flush=True)
        return [], set()

    for plan in plans:
        reasons = list(plan.changed_files[:4])
        if len(plan.changed_files) > 4:
            reasons.append(f"+{len(plan.changed_files) - 4} more")
        if plan.manifest_changed:
            reasons.append("Cargo.toml")
        action = "Would bump" if not apply else "Bumping"
        print(
            f"[{label}] {action} {plan.crate.name}: {plan.old_version} -> {plan.new_version} "
            f"(changed: {', '.join(reasons)})",
            flush=True,
        )

    if not apply:
        return plans, set()

    modified_paths: set[Path] = set()
    version_map = {p.crate.name: p.new_version for p in plans}
    for plan in plans:
        if bump_manifest_version(plan.crate.manifest_path, plan.new_version):
            modified_paths.add(plan.crate.manifest_path.resolve())

    ws_toml = (repo_root / "Cargo.toml").resolve()
    if bump_workspace_dependency_versions(ws_toml, version_map):
        modified_paths.add(ws_toml)
    modified_paths.update(bump_explicit_internal_dependency_versions(crates, version_map))

    lock_res = subprocess.run(
        ["cargo", "update", "--workspace", "--offline"],
        cwd=str(repo_root),
        text=True,
        capture_output=True,
        check=False,
    )
    if lock_res.returncode != 0:
        lock_res = subprocess.run(
            ["cargo", "update", "--workspace"],
            cwd=str(repo_root),
            text=True,
            capture_output=True,
            check=False,
        )
    lock_file = (repo_root / "Cargo.lock").resolve()
    if lock_res.returncode == 0 and lock_file.is_file():
        modified_paths.add(lock_file)
    return plans, modified_paths


def topological_sort_crates(crates: list[WorkspaceCrate]) -> list[WorkspaceCrate]:
    """Order workspace crates so workspace dependencies always precede their dependents."""
    by_name = {c.name: c for c in crates}
    ordered: list[WorkspaceCrate] = []
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(name: str) -> None:
        if name in visited or name not in by_name:
            return
        if name in visiting:
            raise ValueError(f"Cycle detected involving crate {name}")
        visiting.add(name)
        for dep_name in by_name[name].workspace_deps:
            visit(dep_name)
        visiting.remove(name)
        visited.add(name)
        ordered.append(by_name[name])

    for crate in crates:
        visit(crate.name)
    return ordered


def partition_publish_targets(
    ready: list[WorkspaceCrate],
    info_map: dict[str, CratesIoInfo],
    unpublished_names: set[str],
    updated_only: bool,
) -> list[WorkspaceCrate]:
    """Select and order crates to publish, placing already-existing crate updates before new (`404`) crates."""
    topo = topological_sort_crates(ready)
    updates: list[WorkspaceCrate] = []
    new_crates: list[WorkspaceCrate] = []
    new_set: set[str] = set()

    for crate in topo:
        info = info_map[crate.name]
        # A crate can safely publish in the fast "updates" batch only if it already exists on
        # crates.io AND does not depend on any unpublished brand-new crate.
        depends_on_new = any(
            dep in new_set or (dep in unpublished_names and not info_map[dep].is_strataui_repo)
            for dep in crate.workspace_deps
        )
        if info.is_strataui_repo and not depends_on_new:
            updates.append(crate)
        else:
            new_crates.append(crate)
            new_set.add(crate.name)

    if updated_only:
        return updates
    return updates + new_crates


def parse_rate_limit(output: str, now: float | None = None) -> tuple[bool, float | None, str | None]:
    """Detect crates.io 429 rate limiting in `cargo publish` output.

    Returns `(is_rate_limited, wait_seconds, human_summary)`.
    `wait_seconds` is computed directly from the server's `Please try again after <date>`
    timestamp (or `Retry-After` seconds) plus `CLOCK_SKEW_BUFFER_SECS`.
    """
    is_429 = (
        "status 429 Too Many Requests" in output
        or "429 Too Many Requests" in output
        or "too many new crates in a short period of time" in output.lower()
        or "rate-limits" in output.lower()
    )
    if not is_429:
        return False, None, None

    current_ts = time.time() if now is None else now

    date_matches = RETRY_AFTER_DATE_RE.findall(output)
    if date_matches:
        raw_date = date_matches[-1]
        try:
            retry_dt = email.utils.parsedate_to_datetime(raw_date)
            if retry_dt.tzinfo is None:
                retry_dt = retry_dt.replace(tzinfo=dt.timezone.utc)
            delta = max(0.0, retry_dt.timestamp() - current_ts)
            wait_secs = delta + CLOCK_SKEW_BUFFER_SECS
            local_str = retry_dt.astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
            utc_str = retry_dt.astimezone(dt.timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
            return True, wait_secs, f"reset at {utc_str} ({local_str}); in {delta:.0f}s (+{CLOCK_SKEW_BUFFER_SECS:.0f}s buffer)"
        except Exception:
            pass

    iso_matches = RETRY_AFTER_ISO_RE.findall(output)
    if iso_matches:
        raw_iso = iso_matches[-1].replace("Z", "+00:00")
        try:
            retry_dt = dt.datetime.fromisoformat(raw_iso)
            if retry_dt.tzinfo is None:
                retry_dt = retry_dt.replace(tzinfo=dt.timezone.utc)
            delta = max(0.0, retry_dt.timestamp() - current_ts)
            wait_secs = delta + CLOCK_SKEW_BUFFER_SECS
            local_str = retry_dt.astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
            utc_str = retry_dt.astimezone(dt.timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
            return True, wait_secs, f"reset at {utc_str} ({local_str}); in {delta:.0f}s (+{CLOCK_SKEW_BUFFER_SECS:.0f}s buffer)"
        except Exception:
            pass

    sec_matches = RETRY_AFTER_SECS_RE.findall(output)
    if sec_matches:
        secs = float(sec_matches[-1])
        wait_secs = secs + CLOCK_SKEW_BUFFER_SECS
        return True, wait_secs, f"retry after {secs:.0f}s (+{CLOCK_SKEW_BUFFER_SECS:.0f}s buffer)"

    return True, FALLBACK_RATE_LIMIT_SECS, f"no reset timestamp in response; using fallback {FALLBACK_RATE_LIMIT_SECS:.0f}s"


def _format_duration(seconds: float) -> str:
    """Format a non-negative duration cleanly as `XhYYmZZs` or `MmSSs`."""
    total_secs = max(0, int(round(seconds)))
    hours, rem = divmod(total_secs, 3600)
    mins, secs = divmod(rem, 60)
    if hours > 0:
        return f"{hours}h{mins:02d}m{secs:02d}s"
    return f"{mins}m{secs:02d}s"


def parse_deleted_name_cooldown(
    output: str,
    now: float | None = None,
) -> tuple[bool, str | None, float | None, str | None]:
    """Detect crates.io 24h post-deletion name reuse cooldown in `cargo publish` output.

    Returns `(is_cooldown, crate_name, available_epoch_ts, human_summary)`.
    `available_epoch_ts` includes `CLOCK_SKEW_BUFFER_SECS`.
    """
    matches = DELETED_CRATE_REUSE_RE.findall(output)
    if not matches:
        return False, None, None, None

    deleted_name, raw_iso = matches[-1]
    failed_matches = FAILED_PUBLISH_CRATE_RE.findall(output)
    reported_name = failed_matches[-1] if failed_matches else deleted_name

    current_ts = time.time() if now is None else now
    try:
        avail_dt = dt.datetime.fromisoformat(raw_iso.replace("Z", "+00:00"))
        if avail_dt.tzinfo is None:
            avail_dt = avail_dt.replace(tzinfo=dt.timezone.utc)
        delta = max(0.0, avail_dt.timestamp() - current_ts)
        available_ts = max(avail_dt.timestamp(), current_ts) + CLOCK_SKEW_BUFFER_SECS
        local_str = avail_dt.astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
        utc_str = avail_dt.astimezone(dt.timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
        summary = (
            f"available at {utc_str} ({local_str}); in {_format_duration(delta)} "
            f"(+{CLOCK_SKEW_BUFFER_SECS:.0f}s buffer)"
        )
        return True, reported_name or deleted_name, available_ts, summary
    except Exception:
        fallback_ts = current_ts + FALLBACK_RATE_LIMIT_SECS
        return (
            True,
            reported_name or deleted_name,
            fallback_ts,
            f"unparseable reuse timestamp {raw_iso!r}; using fallback {_format_duration(FALLBACK_RATE_LIMIT_SECS)}",
        )


def _match_crate_by_canonical_name(crates: list[WorkspaceCrate], raw_name: str | None) -> str | None:
    """Resolve a crates.io crate name (matching `-` and `_` equivalently) to a workspace crate name."""
    if not raw_name:
        return None
    target_canon = raw_name.lower().replace("-", "_")
    for crate in crates:
        if crate.name.lower().replace("-", "_") == target_canon:
            return crate.name
    return None


def collect_cooldown_blocked(
    unpublished: list[WorkspaceCrate],
    active_cooldowns: dict[str, float],
) -> set[str]:
    """Return unpublished crate names currently in deletion cooldown or transitively depending on one."""
    if not active_cooldowns:
        return set()
    unpub_by_name = {c.name: c for c in unpublished}
    rev_deps: dict[str, set[str]] = {}
    for crate in unpublished:
        for dep in crate.workspace_deps:
            if dep in unpub_by_name:
                rev_deps.setdefault(dep, set()).add(crate.name)

    blocked: set[str] = {name for name in active_cooldowns if name in unpub_by_name}
    queue = list(blocked)
    while queue:
        cur = queue.pop()
        for dependent in rev_deps.get(cur, ()):
            if dependent not in blocked:
                blocked.add(dependent)
                queue.append(dependent)
    return blocked


def stream_command(cmd: list[str], cwd: Path) -> tuple[int, str]:
    """Run `cmd` in `cwd`, streaming combined stdout/stderr live while capturing full output."""
    proc = subprocess.Popen(
        cmd,
        cwd=str(cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    chunks: list[str] = []
    assert proc.stdout is not None
    for line in proc.stdout:
        sys.stdout.write(line)
        sys.stdout.flush()
        chunks.append(line)
    rc = proc.wait()
    return rc, "".join(chunks)


def sleep_with_countdown(seconds: float, reason: str = "rate-limit") -> None:
    """Sleep for `seconds` while printing periodic status updates."""
    end = time.time() + max(0.0, seconds)
    while True:
        remaining = end - time.time()
        if remaining <= 0:
            break
        if remaining > 3600.0:
            interval = 900.0
        elif remaining > 600.0:
            interval = 300.0
        else:
            interval = 60.0
        step = min(remaining, interval)
        time.sleep(step)
        remaining_after = end - time.time()
        if remaining_after > 1.0:
            print(
                f"[{reason}] {_format_duration(remaining_after)} remaining before next publish attempt...",
                flush=True,
            )


def find_canonical_name_collisions(crates: list[WorkspaceCrate]) -> dict[str, list[str]]:
    """Return canonical (`-` vs `_` normalized) crate name collisions among publishable crates."""
    by_canon: dict[str, list[str]] = {}
    for crate in crates:
        canon = crate.name.lower().replace("-", "_")
        by_canon.setdefault(canon, []).append(crate.name)
    return {canon: sorted(names) for canon, names in by_canon.items() if len(names) > 1}


def publish_workspace(
    label: str,
    repo_root: Path,
    extra_args: list[str],
    single_package: str | None = None,
    *,
    auto_bump: bool = True,
    bump_only: bool = False,
    updated_only: bool = False,
    cache_dir: Path | None = None,
    ignore_paths_by_repo: dict[Path, set[Path]] | None = None,
) -> int:
    """Auto-bump changed clean crates (when enabled) and publish unpublished clean crates in `repo_root`.

    Skips any crate that has uncommitted or untracked files in git, as well as every crate that
    transitively depends on a dirty crate (including external path dependencies in sibling repos),
    unless `--allow-dirty` is explicitly passed in `extra_args`. Crates blocked by crates.io's
    24-hour post-deletion name cooldown are skipped while other ready crates publish, then retried
    automatically once their cooldown expires.
    """
    dry_run = "--dry-run" in extra_args
    allow_dirty = "--allow-dirty" in extra_args
    resolved_cache = cache_dir or (repo_root / "target" / ".crates_io_publish_cache")

    crates = collect_workspace_crates(repo_root)
    if single_package is not None:
        crates = [c for c in crates if c.name == single_package]

    collisions = find_canonical_name_collisions(crates)
    if collisions:
        details = "; ".join(f"{canon}: {', '.join(names)}" for canon, names in sorted(collisions.items()))
        print(
            f"[error] [{label}] Crates.io canonical `-`/`_` name collision(s) detected: {details}",
            file=sys.stderr,
            flush=True,
        )
        return 1

    considered_names = {c.name for c in crates}

    if allow_dirty:
        dirty_report = DirtyCrateReport(frozenset(), frozenset(), {})
    else:
        dirty_report = collect_dirty_crates(
            repo_root,
            ignore_paths_by_repo=ignore_paths_by_repo,
        )

    last_reported_dirty = dirty_report.all_dirty & considered_names
    summary = _format_dirty_crates_summary(label, considered_names, dirty_report)
    if summary:
        print(summary, flush=True)

    clean_crates = [c for c in crates if c.name not in dirty_report.all_dirty]
    if not clean_crates:
        skipped_dirty = len(crates)
        if skipped_dirty:
            print(
                f"[{label}] No clean crates left to publish ({skipped_dirty} dirty/dependent crate(s) skipped).",
                flush=True,
            )
        else:
            print(f"[{label}] No publishable crates found.", flush=True)
        return 0

    info_map = {c.name: fetch_crates_io_info(c.name, cache_dir=resolved_cache) for c in clean_crates}
    bumped_paths: set[Path] = set()

    if auto_bump:
        plans, bumped_paths = auto_bump_changed_crates(
            label,
            repo_root,
            clean_crates,
            info_map,
            resolved_cache,
            apply=not dry_run,
        )
        if bumped_paths and ignore_paths_by_repo is not None:
            ignore_paths_by_repo.setdefault(repo_root.resolve(), set()).update(bumped_paths)
        if bump_only:
            return 0
        if plans and not dry_run:
            crates = collect_workspace_crates(repo_root)
            if single_package is not None:
                crates = [c for c in crates if c.name == single_package]
            considered_names = {c.name for c in crates}

    if bump_only:
        return 0

    attempt = 0
    propagation_retries = 0
    needs_info_refresh = False
    cooldown_until: dict[str, float] = {}

    while True:
        attempt += 1
        if allow_dirty:
            dirty_report = DirtyCrateReport(frozenset(), frozenset(), {})
        else:
            dirty_report = collect_dirty_crates(
                repo_root,
                ignore_paths=bumped_paths,
                ignore_paths_by_repo=ignore_paths_by_repo,
            )
            current_reported_dirty = dirty_report.all_dirty & considered_names
            if current_reported_dirty != last_reported_dirty:
                last_reported_dirty = current_reported_dirty
                summary = _format_dirty_crates_summary(label, considered_names, dirty_report)
                if summary:
                    print(summary, flush=True)

        clean_crates = [c for c in crates if c.name not in dirty_report.all_dirty]
        skipped_dirty = len(crates) - len(clean_crates)
        if not clean_crates:
            print(
                f"[{label}] No clean crates left to publish ({skipped_dirty} dirty/dependent crate(s) skipped).",
                flush=True,
            )
            return 0

        if needs_info_refresh:
            info_map = {c.name: fetch_crates_io_info(c.name, cache_dir=resolved_cache) for c in clean_crates}
            needs_info_refresh = False
        else:
            for crate in clean_crates:
                if crate.name not in info_map:
                    info_map[crate.name] = fetch_crates_io_info(crate.name, cache_dir=resolved_cache)

        published: list[WorkspaceCrate] = []
        unpublished: list[WorkspaceCrate] = []
        for crate in clean_crates:
            info = info_map[crate.name]
            if info.is_strataui_repo and crate.version in info.active_versions:
                published.append(crate)
            elif info.exists and not info.is_strataui_repo:
                print(
                    f"[error] Crate name `{crate.name}` already exists on crates.io under a non-strataui repo.",
                    file=sys.stderr,
                    flush=True,
                )
                return 1
            else:
                unpublished.append(crate)

        if not unpublished:
            if skipped_dirty:
                print(
                    f"[{label}] All {len(clean_crates)} clean crate(s) are already published on crates.io "
                    f"({skipped_dirty} dirty/dependent crate(s) skipped).",
                    flush=True,
                )
            else:
                print(f"[{label}] All {len(clean_crates)} crate(s) are already published on crates.io.", flush=True)
            return 0

        unpub_names = {c.name for c in unpublished}
        now_ts = time.time()
        for name in list(cooldown_until):
            if name not in unpub_names or cooldown_until[name] <= now_ts:
                del cooldown_until[name]

        cooldown_blocked = collect_cooldown_blocked(unpublished, cooldown_until)
        ready_unpublished = [c for c in unpublished if c.name not in cooldown_blocked]
        targets = partition_publish_targets(ready_unpublished, info_map, unpub_names, updated_only)

        if not targets:
            if updated_only and not any(
                info_map[c.name].is_strataui_repo for c in unpublished if c.name in cooldown_blocked
            ):
                print(
                    f"[{label}] No updated existing crates to publish (--updated-only skipped {len(unpublished)} brand-new crate(s)).",
                    flush=True,
                )
                return 0
            if cooldown_blocked and cooldown_until:
                earliest_name, earliest_ts = min(cooldown_until.items(), key=lambda item: item[1])
                wait_secs = max(0.0, earliest_ts - time.time())
                earliest_dt = dt.datetime.fromtimestamp(earliest_ts, tz=dt.timezone.utc)
                utc_str = earliest_dt.strftime("%Y-%m-%d %H:%M:%S UTC")
                local_str = earliest_dt.astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
                print(
                    f"[{label}] {len(cooldown_blocked)} crate(s) waiting for crates.io deletion cooldown "
                    f"(earliest unlock: `{earliest_name}` at {utc_str} / {local_str}; "
                    f"waiting {_format_duration(wait_secs)} before retrying)...",
                    flush=True,
                )
                if dry_run:
                    return 0
                sleep_with_countdown(wait_secs, reason="cooldown")
                needs_info_refresh = True
                propagation_retries = 0
                continue
            return 0

        skip_msg = f"skipping {len(published)} already published"
        if updated_only and len(ready_unpublished) > len(targets):
            skip_msg += f", {len(ready_unpublished) - len(targets)} brand-new skipped"
        if skipped_dirty:
            skip_msg += f", {skipped_dirty} dirty/dependent skipped"
        if cooldown_blocked:
            skip_msg += f", {len(cooldown_blocked)} in deletion cooldown"
        print(
            f"[{label}] Attempt {attempt}: {len(targets)} crate(s) ready ({skip_msg}): "
            f"{', '.join(f'{c.name}@{c.version}' for c in targets[:8])}"
            + ("..." if len(targets) > 8 else ""),
            flush=True,
        )

        run_args = list(extra_args)
        if bumped_paths and "--allow-dirty" not in run_args:
            # Allow clean auto-bumped manifests/lockfile during workspace publish.
            run_args.append("--allow-dirty")

        pkg_flags: list[str] = []
        for target in targets:
            pkg_flags.extend(["-p", target.name])
        cmd = ["cargo", "publish", *pkg_flags, *run_args]

        rc, output = stream_command(cmd, repo_root)
        made_upload_progress = ("Uploaded " in output) or ("Published " in output)
        if rc == 0:
            if not cooldown_blocked:
                return 0
            print(
                f"[{label}] Published unblocked crate(s); {len(cooldown_blocked)} crate(s) still waiting on deletion cooldown...",
                flush=True,
            )
            needs_info_refresh = True
            propagation_retries = 0
            continue

        deleted_hit, deleted_name, available_ts, cooldown_summary = parse_deleted_name_cooldown(output)
        if deleted_hit and not dry_run:
            assert available_ts is not None
            matched_name = _match_crate_by_canonical_name(targets, deleted_name)
            if matched_name is not None:
                cooldown_until[matched_name] = available_ts
                print(
                    f"\n[{label}] Skipping `{matched_name}` for now: recently deleted on crates.io "
                    f"({cooldown_summary}); continuing with other ready crates and will retry later.",
                    flush=True,
                )
                if made_upload_progress:
                    needs_info_refresh = True
                    propagation_retries = 0
                continue
            wait_secs = max(0.0, available_ts - time.time())
            print(
                f"\n[{label}] Detected crates.io deletion cooldown ({cooldown_summary}). "
                f"Sleeping {_format_duration(wait_secs)} before retrying...",
                flush=True,
            )
            sleep_with_countdown(wait_secs, reason="cooldown")
            needs_info_refresh = True
            propagation_retries = 0
            continue

        limited, wait_secs, summary = parse_rate_limit(output)
        if limited and not dry_run:
            assert wait_secs is not None
            print(
                f"\n[{label}] Detected crates.io HTTP 429 rate limit ({summary}). "
                f"Sleeping {_format_duration(wait_secs)} before retrying...",
                flush=True,
            )
            sleep_with_countdown(wait_secs, reason="rate-limit")
            needs_info_refresh = True
            propagation_retries = 0
            continue

        if (
            not dry_run
            and propagation_retries < 4
            and (
                "failed to select a version for the requirement" in output
                or "no matching package named" in output
            )
        ):
            propagation_retries += 1
            print(
                f"\n[{label}] Crates.io index has not yet propagated newly published dependencies; "
                f"retrying in 10s (retry {propagation_retries}/4)...",
                flush=True,
            )
            time.sleep(10.0)
            needs_info_refresh = True
            continue

        return rc


def parse_args(argv: list[str]) -> tuple[argparse.Namespace, list[str]]:
    """Parse script control flags while forwarding unknown flags to `cargo publish`."""
    parser = argparse.ArgumentParser(
        description=(
            "Auto-bump changed crates since their last publish and publish workspace crates to crates.io. "
            "Crates with uncommitted or untracked files (and crates that transitively depend on them) "
            "are skipped unless `--allow-dirty` is passed."
        ),
    )
    parser.add_argument(
        "--bump-only",
        action="store_true",
        help="Only detect changed published crates and bump versions/manifests, without running `cargo publish`.",
    )
    parser.add_argument(
        "--no-bump",
        action="store_true",
        help="Skip automatic changed-crate version bumping.",
    )
    parser.add_argument(
        "--updated-only",
        action="store_true",
        help="Only publish crates that already exist on crates.io (skip brand-new 404 crates).",
    )
    parser.add_argument(
        "--skip-earth",
        "--no-earth",
        action="store_true",
        dest="skip_earth",
        help="Skip the companion `earth` (`strata-earth`) repository.",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=None,
        help="Optional directory to cache downloaded `.crate` archives from static.crates.io.",
    )
    return parser.parse_known_args(argv)


def main(argv: list[str] | None = None) -> int:
    args, extra_args = parse_args(sys.argv[1:] if argv is None else argv)
    script_dir = Path(__file__).resolve().parent
    strata_root = script_dir.parent
    earth_root = strata_root.parent / "earth"
    bumped_paths_by_repo: dict[Path, set[Path]] = {}

    rc = publish_workspace(
        "strata",
        strata_root,
        extra_args,
        auto_bump=not args.no_bump,
        bump_only=args.bump_only,
        updated_only=args.updated_only,
        cache_dir=args.cache_dir,
        ignore_paths_by_repo=bumped_paths_by_repo,
    )
    if rc != 0:
        return rc

    if not args.skip_earth and earth_root.is_dir():
        rc = publish_workspace(
            "earth",
            earth_root,
            extra_args,
            single_package="strata-earth",
            auto_bump=not args.no_bump,
            bump_only=args.bump_only,
            updated_only=args.updated_only,
            cache_dir=args.cache_dir,
            ignore_paths_by_repo=bumped_paths_by_repo,
        )
        if rc != 0:
            return rc

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        raise SystemExit(130)
