# Homebrew formula for Kivo's macOS Apple Silicon build.
# Not installable until a macOS asset is published and the placeholder hash is replaced.
# Then install through an explicit tap (the repository is not named homebrew-kivo):
#   brew tap 0libote/kivo https://github.com/0libote/Kivo.git
#   brew install 0libote/kivo/kivo
# Update with `brew update` followed by `brew upgrade 0libote/kivo/kivo`.
# Per release, bump `version`, `url`, and `sha256` (see README's Release section).
class Kivo < Formula
  desc "Quiet dictation and writing utility"
  homepage "https://github.com/0libote/Kivo"
  url "https://github.com/0libote/Kivo/releases/download/0.1.0/kivo-native-macos-arm64"
  version "0.1.0"
  # shortcut: placeholder hash until a release publishes a macOS asset, then paste the real sha256.
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  depends_on arch: :arm64

  def install
    bin.install "kivo-native-macos-arm64" => "kivo"
  end

  test do
    assert_predicate bin/"kivo", :exist?
  end
end
