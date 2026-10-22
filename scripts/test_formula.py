import re
import unittest
from pathlib import Path

FORMULA = Path(__file__).resolve().parents[1] / "Formula" / "kivo.rb"


class FormulaTests(unittest.TestCase):
    def setUp(self):
        self.text = FORMULA.read_text(encoding="utf-8")

    def test_installs_pinned_macos_asset_as_kivo(self):
        version = re.search(r'^  version "([^"]+)"', self.text, re.M).group(1)
        self.assertIn(
            f"releases/download/{version}/kivo-native-macos-arm64", self.text
        )
        self.assertRegex(self.text, r'(?m)^  sha256 "[0-9a-f]{64}"')
        self.assertIn("depends_on arch: :arm64", self.text)
        self.assertIn('bin.install "kivo-native-macos-arm64" => "kivo"', self.text)

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
