#!/usr/bin/env python3
"""Idempotently publish all unpublished Strata workspace crates and strata-earth to crates.io.

Queries crates.io to exclude crate versions already published by strataui, runs
`cargo publish`, and automatically detects HTTP 429 rate-limit responses—parsing
the exact reset timestamp (`Please try again after ...`) or retry delay from
the server error message and sleeping until the window reopens.

Requires `cargo login` or `CARGO_REGISTRY_TOKEN`. Any CLI arguments are
forwarded directly to `cargo publish` (e.g. `--dry-run`, `--allow-dirty`).
"""

from __future__ import annotations

import datetime as dt
import email.utils
import glob
import json
import os
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

USER_AGENT = "strata-publish/2.0"
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
CLOCK_SKEW_BUFFER_SECS = 3.0
FALLBACK_RATE_LIMIT_SECS = 600.0


def workspace_version(workspace_toml: Path) -> str:
    """Return `[workspace.package].version` from a workspace Cargo.toml."""
    with workspace_toml.open("rb") as f:
        data = tomllib.load(f)
    ver = data.get("workspace", {}).get("package", {}).get("version")
    return ver if isinstance(ver, str) else "0.1.0"


def collect_publishable_crates(repo_root: Path) -> list[tuple[str, str]]:
    """Return sorted `(crate_name, version)` pairs for publishable crates under `repo_root/crates`."""
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


def collect_dirty_crates(repo_root: Path) -> set[str]:
    """Return crate names that have uncommitted changes or transitively depend on one."""
    res = subprocess.run(
        ["git", "-C", str(repo_root), "status", "--porcelain", "--untracked-files=all"],
        text=True,
        capture_output=True,
        check=False,
    )
    if res.returncode != 0 or not res.stdout.strip():
        return set()

    dirty_roots: set[str] = set()
    for line in res.stdout.splitlines():
        if len(line) < 4:
            continue
        raw_path = line[3:].strip()
        if " -> " in raw_path:
            raw_path = raw_path.split(" -> ", 1)[1].strip()
        raw_path = raw_path.strip('"')
        p = (repo_root / raw_path).resolve()
        cur = p if p.is_dir() else p.parent
        while cur != repo_root and cur != cur.parent:
            manifest = cur / "Cargo.toml"
            if manifest.is_file():
                try:
                    with manifest.open("rb") as f:
                        name = tomllib.load(f).get("package", {}).get("name")
                    if isinstance(name, str) and name:
                        dirty_roots.add(name)
                except Exception:
                    pass
                break
            cur = cur.parent

    if not dirty_roots:
        return set()

    # Expand to transitive workspace dependents so clean crates that depend on dirty ones are also deferred.
    meta = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=str(repo_root),
        text=True,
        capture_output=True,
        check=False,
    )
    if meta.returncode != 0:
        return dirty_roots

    try:
        packages = json.loads(meta.stdout).get("packages", [])
        rev_deps: dict[str, set[str]] = {}
        for pkg in packages:
            pkg_name = pkg.get("name")
            if not pkg_name:
                continue
            for dep in pkg.get("dependencies", []):
                dep_name = dep.get("name")
                if dep_name:
                    rev_deps.setdefault(dep_name, set()).add(pkg_name)

        all_dirty = set(dirty_roots)
        queue = list(dirty_roots)
        while queue:
            cur_pkg = queue.pop()
            for dependent in rev_deps.get(cur_pkg, ()):
                if dependent not in all_dirty:
                    all_dirty.add(dependent)
                    queue.append(dependent)
        return all_dirty
    except Exception:
        return dirty_roots


def is_published_on_crates_io(name: str, version: str) -> bool:
    """Return True if `name` at `version` is already published under a `strataui` repository on crates.io."""
    url = f"https://crates.io/api/v1/crates/{name}"
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read().decode())
        repo = (data.get("crate") or {}).get("repository") or ""
        versions = {v["num"] for v in data.get("versions", []) if not v.get("yanked")}
        return "strataui" in repo and version in versions
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return False
        print(f"[warn] crates.io lookup for {name} returned HTTP {e.code}", file=sys.stderr, flush=True)
        return False
    except Exception as e:
        print(f"[warn] crates.io lookup for {name} failed: {e}", file=sys.stderr, flush=True)
        return False


def split_published(crates: list[tuple[str, str]]) -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    """Partition `crates` into `(published, unpublished)` lists by querying crates.io."""
    published: list[tuple[str, str]] = []
    unpublished: list[tuple[str, str]] = []
    for name, ver in crates:
        if is_published_on_crates_io(name, ver):
            published.append((name, ver))
        else:
            unpublished.append((name, ver))
    return published, unpublished


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


def sleep_with_countdown(seconds: float) -> None:
    """Sleep for `seconds` while printing periodic status updates every 60 seconds."""
    end = time.time() + max(0.0, seconds)
    while True:
        remaining = end - time.time()
        if remaining <= 0:
            break
        step = min(remaining, 60.0)
        time.sleep(step)
        remaining_after = end - time.time()
        if remaining_after > 1.0:
            mins, secs = divmod(int(round(remaining_after)), 60)
            print(f"[rate-limit] {mins}m{secs:02d}s remaining before next publish attempt...", flush=True)


def publish_workspace(
    label: str,
    repo_root: Path,
    extra_args: list[str],
    single_package: str | None = None,
) -> int:
    """Publish all unpublished crates in `repo_root`, automatically waiting out crates.io 429 limits."""
    dry_run = "--dry-run" in extra_args
    all_crates = collect_publishable_crates(repo_root)
    if single_package is not None:
        all_crates = [c for c in all_crates if c[0] == single_package]

    attempt = 0
    while True:
        attempt += 1
        published, unpublished = split_published(all_crates)
        if not unpublished:
            print(f"[{label}] All {len(all_crates)} crate(s) are already published on crates.io.", flush=True)
            return 0

        dirty_names = set() if "--allow-dirty" in extra_args else collect_dirty_crates(repo_root)
        ready = [(n, v) for n, v in unpublished if n not in dirty_names]
        deferred = [(n, v) for n, v in unpublished if n in dirty_names]

        if not ready and deferred:
            print(
                f"[{label}] Waiting 15s for {len(deferred)} crate(s) with uncommitted changes to be committed: "
                f"{', '.join(f'{n}@{v}' for n, v in deferred[:8])}"
                + ("..." if len(deferred) > 8 else ""),
                flush=True,
            )
            if dry_run:
                return 0
            time.sleep(15.0)
            continue

        skip_msg = f"skipping {len(published)} already published"
        if deferred:
            skip_msg += f", deferring {len(deferred)} uncommitted"
        print(
            f"[{label}] Attempt {attempt}: {len(ready)} crate(s) ready ({skip_msg}): "
            f"{', '.join(f'{n}@{v}' for n, v in ready[:8])}"
            + ("..." if len(ready) > 8 else ""),
            flush=True,
        )

        run_args = list(extra_args)
        if deferred and "--allow-dirty" not in run_args:
            # Workspace-wide git dirty check would otherwise block clean crates when unrelated crates are dirty.
            run_args.append("--allow-dirty")

        if single_package is not None:
            cmd = ["cargo", "publish", "-p", single_package, *run_args]
        else:
            excludes: list[str] = []
            for name, _ in published:
                excludes.extend(["--exclude", name])
            for name, _ in deferred:
                excludes.extend(["--exclude", name])
            cmd = ["cargo", "publish", "--workspace", *excludes, *run_args]

        rc, output = stream_command(cmd, repo_root)
        if rc == 0:
            if not deferred:
                return 0
            print(
                f"[{label}] Clean crates published; revisiting {len(deferred)} deferred crate(s)...",
                flush=True,
            )
            continue

        limited, wait_secs, summary = parse_rate_limit(output)
        if limited and not dry_run:
            assert wait_secs is not None
            print(
                f"\n[{label}] Detected crates.io HTTP 429 rate limit ({summary}). "
                f"Sleeping {wait_secs:.1f}s before retrying...",
                flush=True,
            )
            sleep_with_countdown(wait_secs)
            continue

        if (
            single_package is not None
            and not dry_run
            and attempt <= 4
            and (
                "failed to select a version for the requirement" in output
                or "no matching package named" in output
            )
        ):
            print(
                f"\n[{label}] Crates.io index has not yet propagated newly published strata dependencies; "
                f"retrying in 10s (attempt {attempt}/4)...",
                flush=True,
            )
            time.sleep(10.0)
            continue

        return rc


def main() -> int:
    extra_args = sys.argv[1:]
    script_dir = Path(__file__).resolve().parent
    strata_root = script_dir.parent
    earth_root = strata_root.parent / "earth"

    rc = publish_workspace("strata", strata_root, extra_args)
    if rc != 0:
        return rc

    if earth_root.is_dir():
        rc = publish_workspace("earth", earth_root, extra_args, single_package="strata-earth")
        if rc != 0:
            return rc

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        raise SystemExit(130)
