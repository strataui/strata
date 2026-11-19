#!/usr/bin/env python3
"""Unit tests for `publish_crates.py` version bumping, diffing, ordering, and CLI handling."""

from __future__ import annotations

import contextlib
import io
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import publish_crates  # noqa: E402


def _make_crate_tarball(name: str, version: str, files: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for rel, content in files.items():
            info = tarfile.TarInfo(name=f"{name}-{version}/{rel}")
            info.size = len(content)
            tf.addfile(info, io.BytesIO(content))
    return buf.getvalue()


class PublishCratesTests(unittest.TestCase):
    def test_bump_patch_version_skips_existing(self) -> None:
        self.assertEqual(publish_crates.bump_patch_version("0.1.0"), "0.1.1")
        self.assertEqual(
            publish_crates.bump_patch_version("0.1.0", forbidden=frozenset({"0.1.1", "0.1.2"})),
            "0.1.3",
        )

    def test_bump_manifest_and_workspace_dependencies(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ws_toml = root / "Cargo.toml"
            ws_toml.write_text(
                '[workspace]\nmembers = ["crates/a", "crates/b"]\n\n'
                '[workspace.package]\nversion = "0.1.0"\n\n'
                '[workspace.dependencies]\n'
                'crate_a = { version = "0.1.0", path = "crates/a" }\n'
                'crate_b_alias = { package = "crate-b", path = "crates/b", version = "0.1.0" }\n'
                'untouched = { version = "0.1.0", path = "crates/untouched" }\n',
                encoding="utf-8",
            )
            a_dir = root / "crates" / "a"
            b_dir = root / "crates" / "b"
            a_dir.mkdir(parents=True)
            b_dir.mkdir(parents=True)
            (a_dir / "Cargo.toml").write_text(
                '[package]\nname = "crate_a"\nversion.workspace = true\n',
                encoding="utf-8",
            )
            (b_dir / "Cargo.toml").write_text(
                '[package]\nname = "crate-b"\nversion = "0.1.0"\n',
                encoding="utf-8",
            )

            self.assertTrue(publish_crates.bump_manifest_version(a_dir / "Cargo.toml", "0.1.1"))
            self.assertTrue(publish_crates.bump_manifest_version(b_dir / "Cargo.toml", "0.1.2"))
            self.assertTrue(
                publish_crates.bump_workspace_dependency_versions(
                    ws_toml,
                    {"crate_a": "0.1.1", "crate-b": "0.1.2"},
                )
            )

            ws_data = tomllib.loads(ws_toml.read_text(encoding="utf-8"))
            self.assertEqual(ws_data["workspace"]["package"]["version"], "0.1.0")
            self.assertEqual(ws_data["workspace"]["dependencies"]["crate_a"]["version"], "0.1.1")
            self.assertEqual(ws_data["workspace"]["dependencies"]["crate_b_alias"]["version"], "0.1.2")
            self.assertEqual(ws_data["workspace"]["dependencies"]["untouched"]["version"], "0.1.0")
            self.assertEqual(
                tomllib.loads((a_dir / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"],
                "0.1.1",
            )
            self.assertEqual(
                tomllib.loads((b_dir / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"],
                "0.1.2",
            )

    def test_has_crate_changed_since_publish_compares_archive_and_local_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            pkg_dir = root / "crates" / "demo"
            (pkg_dir / "src").mkdir(parents=True)
            manifest = pkg_dir / "Cargo.toml"
            manifest.write_text('[package]\nname = "demo"\nversion.workspace = true\n', encoding="utf-8")
            (pkg_dir / "src" / "lib.rs").write_text("pub fn answer() -> u32 { 42 }\n", encoding="utf-8")
            crate = publish_crates.WorkspaceCrate(
                name="demo",
                version="0.1.0",
                manifest_path=manifest,
                pkg_dir=pkg_dir,
                workspace_deps=(),
            )
            archive = _make_crate_tarball(
                "demo",
                "0.1.0",
                {
                    "Cargo.toml.orig": b'[package]\nname = "demo"\nversion = "0.1.0"\n',
                    "Cargo.toml": b'[package]\nname = "demo"\nversion = "0.1.0"\n',
                    "src/lib.rs": b"pub fn answer() -> u32 { 42 }\n",
                },
            )
            cache_dir = root / "cache"
            with (
                mock.patch.object(publish_crates, "download_crate_archive", return_value=archive),
                mock.patch.object(
                    publish_crates,
                    "load_local_package_contents",
                    return_value=(
                        {"src/lib.rs": publish_crates._sha256_bytes(b"pub fn answer() -> u32 { 42 }\n")},
                        manifest.read_bytes(),
                    ),
                ),
            ):
                changed, files, man_changed = publish_crates.has_crate_changed_since_publish(root, crate, cache_dir)
                self.assertFalse(changed)
                self.assertEqual(files, ())
                self.assertFalse(man_changed)

            with (
                mock.patch.object(publish_crates, "download_crate_archive", return_value=archive),
                mock.patch.object(
                    publish_crates,
                    "load_local_package_contents",
                    return_value=(
                        {"src/lib.rs": publish_crates._sha256_bytes(b"pub fn answer() -> u32 { 43 }\n")},
                        manifest.read_bytes(),
                    ),
                ),
            ):
                changed, files, man_changed = publish_crates.has_crate_changed_since_publish(root, crate, cache_dir)
                self.assertTrue(changed)
                self.assertEqual(files, ("src/lib.rs",))
                self.assertFalse(man_changed)

    def test_partition_publish_targets_orders_updates_before_new_crates(self) -> None:
        base = Path("/tmp")
        a = publish_crates.WorkspaceCrate("a_new", "0.1.0", base / "a/Cargo.toml", base / "a", ())
        b = publish_crates.WorkspaceCrate("b_upd", "0.1.1", base / "b/Cargo.toml", base / "b", ())
        c = publish_crates.WorkspaceCrate("c_upd_dep_on_new", "0.1.1", base / "c/Cargo.toml", base / "c", ("a_new",))
        d = publish_crates.WorkspaceCrate("d_upd_dep_on_upd", "0.1.1", base / "d/Cargo.toml", base / "d", ("b_upd",))
        info_map = {
            "a_new": publish_crates.CratesIoInfo(False, False, frozenset(), frozenset(), None),
            "b_upd": publish_crates.CratesIoInfo(True, True, frozenset({"0.1.0"}), frozenset({"0.1.0"}), "0.1.0"),
            "c_upd_dep_on_new": publish_crates.CratesIoInfo(True, True, frozenset({"0.1.0"}), frozenset({"0.1.0"}), "0.1.0"),
            "d_upd_dep_on_upd": publish_crates.CratesIoInfo(True, True, frozenset({"0.1.0"}), frozenset({"0.1.0"}), "0.1.0"),
        }
        full = publish_crates.partition_publish_targets([d, c, b, a], info_map, {"a_new", "b_upd", "c_upd_dep_on_new", "d_upd_dep_on_upd"}, updated_only=False)
        self.assertEqual([x.name for x in full], ["b_upd", "d_upd_dep_on_upd", "a_new", "c_upd_dep_on_new"])
        upd_only = publish_crates.partition_publish_targets([d, c, b, a], info_map, {"a_new", "b_upd", "c_upd_dep_on_new", "d_upd_dep_on_upd"}, updated_only=True)
        self.assertEqual([x.name for x in upd_only], ["b_upd", "d_upd_dep_on_upd"])

    def test_parse_rate_limit(self) -> None:
        now = 1763290000.0
        msg = "error: Please try again after Sun, 16 Nov 2025 11:00:00 GMT"
        limited, secs, summary = publish_crates.parse_rate_limit(msg + " status 429 Too Many Requests", now=now)
        self.assertTrue(limited)
        self.assertIsNotNone(secs)
        self.assertIn("UTC", summary or "")

    def test_sparse_index_url_and_archive_repo_extraction(self) -> None:
        self.assertEqual(publish_crates.sparse_index_url("a"), "https://index.crates.io/1/a")
        self.assertEqual(publish_crates.sparse_index_url("ab"), "https://index.crates.io/2/ab")
        self.assertEqual(publish_crates.sparse_index_url("abc"), "https://index.crates.io/3/a/abc")
        self.assertEqual(
            publish_crates.sparse_index_url("strata-ui-platform"),
            "https://index.crates.io/st/ra/strata-ui-platform",
        )
        archive = _make_crate_tarball(
            "strata_render",
            "0.1.0",
            {
                "Cargo.toml": b'[package]\nname = "strata_render"\nversion = "0.1.0"\nrepository = "https://github.com/strataui/strata"\n',
            },
        )
        self.assertEqual(
            publish_crates._extract_archive_repository(archive, "strata_render", "0.1.0"),
            "https://github.com/strataui/strata",
        )

    @staticmethod
    def _git_commit_all(repo_dir: Path, message: str = "init") -> None:
        subprocess.run(["git", "-C", str(repo_dir), "add", "-A"], check=True)
        subprocess.run(
            [
                "git",
                "-C",
                str(repo_dir),
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                message,
            ],
            check=True,
        )

    @classmethod
    def _init_git_repo(cls, repo_dir: Path) -> None:
        subprocess.run(["git", "init", "-q", str(repo_dir)], check=True)
        (repo_dir / ".gitignore").write_text("ignored.tmp\ntarget/\n", encoding="utf-8")
        (repo_dir / "tracked.txt").write_text("initial\n", encoding="utf-8")
        cls._git_commit_all(repo_dir, "init")

    @classmethod
    def _make_workspace_repo(
        cls,
        repo_dir: Path,
        crates_spec: dict[str, list[tuple[str, str]]],
    ) -> None:
        """Create a real git-initialized Cargo workspace with `crates_spec = {name: [(dep_name, rel_path), ...]}`."""
        repo_dir.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", str(repo_dir)], check=True)
        (repo_dir / ".gitignore").write_text("target/\n", encoding="utf-8")
        members = ", ".join(f'"crates/{name}"' for name in crates_spec)
        (repo_dir / "Cargo.toml").write_text(
            f'[workspace]\nresolver = "2"\nmembers = [{members}]\n',
            encoding="utf-8",
        )
        for name, deps in crates_spec.items():
            cdir = repo_dir / "crates" / name
            (cdir / "src").mkdir(parents=True)
            dep_lines = "\n".join(
                f'{dep_name} = {{ path = "{rel_path}", version = "0.1.0" }}'
                for dep_name, rel_path in deps
            )
            (cdir / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n{dep_lines}\n',
                encoding="utf-8",
            )
            (cdir / "src" / "lib.rs").write_text("pub fn ok() {}\n", encoding="utf-8")
        cls._git_commit_all(repo_dir, "init workspace")

    def test_list_dirty_repo_paths_detects_untracked_and_uncommitted_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            self._init_git_repo(repo)
            resolved = repo.resolve()

            # Clean repo + ignored file should not be reported dirty.
            (repo / "ignored.tmp").write_text("ignored\n", encoding="utf-8")
            self.assertEqual(publish_crates.list_dirty_repo_paths(repo), set())

            # Untracked file (including spaces in nested directory).
            untracked = repo / "nested" / "my untracked.rs"
            untracked.parent.mkdir(parents=True)
            untracked.write_text("fn wip() {}\n", encoding="utf-8")
            self.assertEqual(
                publish_crates.list_dirty_repo_paths(repo),
                {resolved / "nested" / "my untracked.rs"},
            )
            untracked.unlink()

            # Unstaged modified tracked file.
            (repo / "tracked.txt").write_text("modified\n", encoding="utf-8")
            self.assertEqual(
                publish_crates.list_dirty_repo_paths(repo),
                {resolved / "tracked.txt"},
            )

            # Staged rename.
            subprocess.run(["git", "-C", str(repo), "checkout", "--", "tracked.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "mv", "tracked.txt", "renamed.txt"], check=True)
            self.assertEqual(
                publish_crates.list_dirty_repo_paths(repo),
                {resolved / "renamed.txt"},
            )

    def test_collect_dirty_crates_propagates_transitively_and_across_repos(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            strata = root / "strata"
            earth = root / "earth"
            self._make_workspace_repo(
                strata,
                {
                    "core_a": [],
                    "mid_b": [("core_a", "../core_a")],
                    "top_c": [("mid_b", "../mid_b")],
                    "indep_d": [],
                },
            )
            self._make_workspace_repo(
                earth,
                {
                    "strata-earth": [("core_a", "../../../strata/crates/core_a")],
                },
            )

            # Non-crate file outside crates/ should not mark any crate dirty.
            (strata / "scripts").mkdir()
            (strata / "scripts" / "helper.py").write_text("# scratch\n", encoding="utf-8")
            self.assertEqual(publish_crates.collect_dirty_crates(strata).all_dirty, frozenset())
            self.assertEqual(publish_crates.collect_dirty_crates(earth).all_dirty, frozenset())

            # Dirtying core_a marks core_a directly dirty, mid_b/top_c transitively dirty,
            # leaves indep_d clean, and marks earth's strata-earth dirty via external path dep.
            (strata / "crates" / "core_a" / "src" / "wip.rs").write_text("pub fn wip() {}\n", encoding="utf-8")
            strata_report = publish_crates.collect_dirty_crates(strata)
            self.assertEqual(strata_report.direct, frozenset({"core_a"}))
            self.assertEqual(strata_report.transitive, frozenset({"mid_b", "top_c"}))
            self.assertNotIn("indep_d", strata_report.all_dirty)

            earth_report = publish_crates.collect_dirty_crates(earth)
            self.assertEqual(earth_report.direct, frozenset())
            self.assertEqual(earth_report.transitive, frozenset({"strata-earth"}))
            self.assertEqual(earth_report.external_causes, {"strata-earth": ("core_a",)})

            # Moving the dirty file to indep_d leaves core_a and strata-earth clean.
            (strata / "crates" / "core_a" / "src" / "wip.rs").unlink()
            (strata / "crates" / "indep_d" / "src" / "wip.rs").write_text("pub fn wip() {}\n", encoding="utf-8")
            self.assertEqual(publish_crates.collect_dirty_crates(strata).all_dirty, frozenset({"indep_d"}))
            self.assertEqual(publish_crates.collect_dirty_crates(earth).all_dirty, frozenset())

            # Dirtying workspace root Cargo.toml marks all workspace crates dirty unless ignored.
            (strata / "crates" / "indep_d" / "src" / "wip.rs").unlink()
            root_toml = strata / "Cargo.toml"
            root_toml.write_text(root_toml.read_text(encoding="utf-8") + "\n# edit\n", encoding="utf-8")
            self.assertEqual(
                publish_crates.collect_dirty_crates(strata).direct,
                frozenset({"core_a", "mid_b", "top_c", "indep_d"}),
            )
            self.assertEqual(
                publish_crates.collect_dirty_crates(
                    strata,
                    ignore_paths={root_toml},
                ).all_dirty,
                frozenset(),
            )
            self.assertEqual(
                publish_crates.collect_dirty_crates(
                    earth,
                    ignore_paths_by_repo={strata.resolve(): {root_toml.resolve()}},
                ).all_dirty,
                frozenset(),
            )

    def test_publish_workspace_skips_dirty_and_dependents_but_publishes_clean_others(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            strata = Path(tmp) / "strata"
            self._make_workspace_repo(
                strata,
                {
                    "core_a": [],
                    "mid_b": [("core_a", "../core_a")],
                    "indep_d": [],
                },
            )
            (strata / "crates" / "core_a" / "src" / "untracked.rs").write_text("fn draft() {}\n", encoding="utf-8")

            info = publish_crates.CratesIoInfo(False, False, frozenset(), frozenset(), None)
            stdout = io.StringIO()
            with (
                contextlib.redirect_stdout(stdout),
                mock.patch.object(publish_crates, "fetch_crates_io_info", return_value=info),
                mock.patch.object(publish_crates, "auto_bump_changed_crates", return_value=([], set())) as bump_mock,
                mock.patch.object(publish_crates, "stream_command", return_value=(0, "")) as cmd_mock,
            ):
                rc = publish_crates.publish_workspace("strata", strata, [])

            self.assertEqual(rc, 0)
            out = stdout.getvalue()
            self.assertIn("1 directly dirty: core_a", out)
            self.assertIn("1 depending on dirty crates: mid_b", out)
            # Only clean crate indep_d is passed to auto_bump and cargo publish (no --allow-dirty needed).
            bumped_crates_arg = bump_mock.call_args.args[2]
            self.assertEqual([c.name for c in bumped_crates_arg], ["indep_d"])
            cmd_mock.assert_called_once_with(["cargo", "publish", "-p", "indep_d"], strata)

    def test_publish_workspace_allows_dirty_crates_when_flag_passed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            strata = Path(tmp) / "strata"
            self._make_workspace_repo(strata, {"demo": []})
            (strata / "crates" / "demo" / "src" / "lib.rs").write_text("pub fn dirty() {}\n", encoding="utf-8")

            info = publish_crates.CratesIoInfo(False, False, frozenset(), frozenset(), None)
            with (
                mock.patch.object(publish_crates, "fetch_crates_io_info", return_value=info),
                mock.patch.object(publish_crates, "stream_command", return_value=(0, "")) as cmd_mock,
            ):
                rc = publish_crates.publish_workspace("strata", strata, ["--allow-dirty"], auto_bump=False)

            self.assertEqual(rc, 0)
            cmd_mock.assert_called_once_with(["cargo", "publish", "-p", "demo", "--allow-dirty"], strata)

    def test_publish_workspace_permits_auto_bumped_files_alongside_unrelated_dirty_crate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            strata = Path(tmp) / "strata"
            self._make_workspace_repo(
                strata,
                {
                    "clean_a": [],
                    "dirty_b": [],
                },
            )
            (strata / "crates" / "dirty_b" / "src" / "wip.rs").write_text("fn wip() {}\n", encoding="utf-8")
            clean_manifest = (strata / "crates" / "clean_a" / "Cargo.toml").resolve()

            info = publish_crates.CratesIoInfo(True, True, frozenset({"0.1.0"}), frozenset({"0.1.0"}), "0.1.0")

            def fake_bump(
                _label: str,
                _root: Path,
                clean_crates: list[publish_crates.WorkspaceCrate],
                *_args: object,
                **_kwargs: object,
            ) -> tuple[list[publish_crates.BumpPlan], set[Path]]:
                self.assertEqual([c.name for c in clean_crates], ["clean_a"])
                publish_crates.bump_manifest_version(clean_manifest, "0.1.1")
                plan = publish_crates.BumpPlan(clean_crates[0], "0.1.0", "0.1.1", ("src/lib.rs",), False)
                return [plan], {clean_manifest}

            with (
                mock.patch.object(publish_crates, "fetch_crates_io_info", return_value=info),
                mock.patch.object(publish_crates, "auto_bump_changed_crates", side_effect=fake_bump),
                mock.patch.object(publish_crates, "stream_command", return_value=(0, "")) as cmd_mock,
            ):
                rc = publish_crates.publish_workspace("strata", strata, [], auto_bump=True)

            self.assertEqual(rc, 0)
            cmd_mock.assert_called_once_with(["cargo", "publish", "-p", "clean_a", "--allow-dirty"], strata)

    def test_publish_workspace_skips_if_unexpected_dirty_file_appears_mid_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            strata = Path(tmp) / "strata"
            self._make_workspace_repo(strata, {"demo": []})
            info = publish_crates.CratesIoInfo(False, False, frozenset(), frozenset(), None)

            def dirty_during_bump(*_args: object, **_kwargs: object) -> tuple[list[publish_crates.BumpPlan], set[Path]]:
                (strata / "crates" / "demo" / "src" / "new_untracked.rs").write_text("fn oops() {}\n", encoding="utf-8")
                return [], set()

            stdout = io.StringIO()
            with (
                contextlib.redirect_stdout(stdout),
                mock.patch.object(publish_crates, "fetch_crates_io_info", return_value=info),
                mock.patch.object(publish_crates, "auto_bump_changed_crates", side_effect=dirty_during_bump),
                mock.patch.object(publish_crates, "stream_command") as cmd_mock,
            ):
                rc = publish_crates.publish_workspace("strata", strata, [], auto_bump=True)

            self.assertEqual(rc, 0)
            self.assertIn("1 directly dirty: demo", stdout.getvalue())
            self.assertIn("No clean crates left to publish", stdout.getvalue())
            cmd_mock.assert_not_called()

    def test_main_publishes_clean_strata_crates_and_skips_affected_earth(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            strata = root / "strata"
            earth = root / "earth"
            self._make_workspace_repo(
                strata,
                {
                    "core_a": [],
                    "dep_on_core": [("core_a", "../core_a")],
                    "indep_clean": [],
                },
            )
            self._make_workspace_repo(
                earth,
                {
                    "strata-earth": [("core_a", "../../../strata/crates/core_a")],
                },
            )
            (strata / "crates" / "core_a" / "src" / "wip.rs").write_text("fn dirty() {}\n", encoding="utf-8")

            scripts_dir = strata / "scripts"
            scripts_dir.mkdir(parents=True)
            fake_script = scripts_dir / "publish_crates.py"
            fake_script.write_text("# stub\n", encoding="utf-8")

            info = publish_crates.CratesIoInfo(False, False, frozenset(), frozenset(), None)
            stdout = io.StringIO()
            with (
                contextlib.redirect_stdout(stdout),
                mock.patch.object(publish_crates, "__file__", str(fake_script)),
                mock.patch.object(publish_crates, "fetch_crates_io_info", return_value=info),
                mock.patch.object(publish_crates, "stream_command", return_value=(0, "")) as cmd_mock,
            ):
                rc = publish_crates.main(["--no-bump"])

            self.assertEqual(rc, 0)
            out = stdout.getvalue()
            self.assertIn("strata-earth <- core_a", out)
            cmd_mock.assert_called_once_with(
                ["cargo", "publish", "-p", "indep_clean"],
                strata.resolve(),
            )


if __name__ == "__main__":
    unittest.main()

