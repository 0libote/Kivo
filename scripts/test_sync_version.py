import json
from pathlib import Path
import tempfile
import tomllib
import unittest

from sync_version import sync_version, version_key


class SyncVersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory, name in (("native-app", "kivo-native"), ("src-tauri", "kivo")):
            path = self.root / directory
            path.mkdir()
            (path / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion = "0.1.0"\n\n'
                '[dependencies.other]\nversion = "2.0"\n'
            )
            (path / "Cargo.lock").write_text(
                f'version = 4\n\n[[package]]\nname = "{name}"\nversion = "0.1.0"\n\n'
                '[[package]]\nname = "other"\nversion = "2.0.1"\nchecksum = "unchanged"\n'
            )
        (self.root / "src-tauri/tauri.conf.json").write_text('{"version": "0.1.0"}\n')

    def snapshot(self):
        return {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}

    def test_updates_both_tag_formats_and_leaves_dependencies_untouched(self):
        for tag in ("app-v1.2.3-rc.1+build.4", "1.2.3"):
            with self.subTest(tag=tag):
                version = sync_version(self.root, tag)
                for directory in ("native-app", "src-tauri"):
                    manifest = tomllib.loads((self.root / directory / "Cargo.toml").read_text())
                    lock = tomllib.loads((self.root / directory / "Cargo.lock").read_text())
                    self.assertEqual(manifest["package"]["version"], version)
                    self.assertEqual(manifest["dependencies"]["other"]["version"], "2.0")
                    self.assertEqual(lock["version"], 4)
                    self.assertEqual(lock["package"][0]["version"], version)
                    self.assertEqual(lock["package"][1]["version"], "2.0.1")
                    self.assertEqual(lock["package"][1]["checksum"], "unchanged")
                self.assertEqual(json.loads((self.root / "src-tauri/tauri.conf.json").read_text())["version"], version)

    def test_rerun_is_idempotent_and_repairs_equal_versions(self):
        sync_version(self.root, "0.1.0", only_newer=True)
        before = self.snapshot()
        sync_version(self.root, "0.1.0", only_newer=True)
        self.assertEqual(self.snapshot(), before)
        (self.root / "src-tauri/tauri.conf.json").write_text('{"version": "0.0.1"}\n')
        sync_version(self.root, "0.1.0", only_newer=True)
        self.assertEqual(self.snapshot(), before)

    def test_no_downgrade_including_prereleases(self):
        before = self.snapshot()
        for tag in ("0.0.9", "0.1.0-rc.1"):
            sync_version(self.root, tag, only_newer=True)
            self.assertEqual(self.snapshot(), before)
        ordered = ["1.0.0-alpha", "1.0.0-alpha.2", "1.0.0-alpha.10", "1.0.0-beta", "1.0.0-rc.1", "1.0.0", "1.1.0"]
        self.assertEqual(sorted(ordered, key=version_key), ordered)

    def test_invalid_tags_never_change_files(self):
        before = self.snapshot()
        for tag in ("", "1.2", "v1.2.3", "1.2.3/unsafe", "01.2.3", "1.2.3-01", "1.2.3;echo oops"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                sync_version(self.root, tag)
            self.assertEqual(self.snapshot(), before)

    def test_malformed_lockfile_fails_before_any_writes(self):
        (self.root / "src-tauri/Cargo.lock").write_text("version = 4\n")
        before = self.snapshot()
        with self.assertRaises(ValueError):
            sync_version(self.root, "1.2.3")
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
