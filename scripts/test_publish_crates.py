#!/usr/bin/env python3
"""Unit tests for `publish_crates.py` version bumping, diffing, ordering, and CLI handling."""

from __future__ import annotations

import io
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


if __name__ == "__main__":
    unittest.main()

