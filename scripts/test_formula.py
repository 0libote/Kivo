import re
import unittest
from pathlib import Path

FORMULA = Path(__file__).resolve().parents[1] / "Formula" / "kivo.rb"


class FormulaTests(unittest.TestCase):
    def setUp(self):
        self.text = FORMULA.read_text(encoding="utf-8")

    def test_builds_pinned_source_as_kivo(self):
        self.assertRegex(
            self.text,
            r'(?m)^  url "https://github.com/0libote/Kivo/archive/[0-9a-f]{40}\.tar\.gz"',
        )
        checksum = re.search(r'^  sha256 "([0-9a-f]{64})"', self.text, re.M).group(1)
        self.assertNotEqual(checksum, "0" * 64)
        self.assertIn('depends_on "rust" => :build', self.text)
        self.assertIn("depends_on :macos", self.text)
        self.assertIn("depends_on arch: :arm64", self.text)
        self.assertIn('"cargo", "build", "--locked", "--release"', self.text)
        self.assertIn('"--manifest-path", "native-app/Cargo.toml"', self.text)
        self.assertIn('"--target-dir", "target"', self.text)
        self.assertIn('bin.install "target/release/kivo-native" => "kivo"', self.text)

    def test_has_metadata_and_offline_test_block(self):
        for line in (
            "class Kivo < Formula",
            'desc "',
            'homepage "https://github.com/0libote/Kivo"',
        ):
            self.assertIn(line, self.text)
        self.assertIn('assert_predicate bin/"kivo", :exist?', self.text)


if __name__ == "__main__":
    unittest.main()
