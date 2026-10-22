# Homebrew formula for Kivo's macOS Apple Silicon source build.
# Install through an explicit tap (the repository is not named homebrew-kivo):
#   brew tap 0libote/kivo https://github.com/0libote/Kivo.git
#   brew install 0libote/kivo/kivo
# Update with `brew update` followed by `brew upgrade 0libote/kivo/kivo`.
# Pin `url` to a macOS-capable source commit and update `version` and `sha256` together.
class Kivo < Formula
  desc "Quiet dictation and writing utility"
  homepage "https://github.com/0libote/Kivo"
  url "https://github.com/0libote/Kivo/archive/608ba92bba4a97f95d501fb1a5fad3063d0bfb55.tar.gz"
  version "0.1.0"
  sha256 "22bd6e30f8621fbac27443023d1e548df11d5953348f3329bcf8b30453086b52"
  depends_on "rust" => :build
  depends_on :macos
  depends_on arch: :arm64

  def install
    system "cargo", "build", "--locked", "--release", "--manifest-path", "native-app/Cargo.toml",
           "--target-dir", "target"
    bin.install "target/release/kivo-native" => "kivo"
  end

  test do
    assert_predicate bin/"kivo", :exist?
  end
end
