# Kivo

Kivo is a dictation and writing utility for Windows and macOS. It is being ported from Tauri to a native Rust UI.
The native build is a development preview. You can paste text into the writing tools and send it to Gemini. Dictation, global shortcuts, system selection capture, and text replacement still need their OS integrations ported.

## Download and use

Published builds appear on [GitHub Releases](https://github.com/0libote/Kivo/releases).
The release workflow builds portable executables: `kivo-native.exe` for Windows 11 24H2+ and `kivo-native-macos-arm64` for macOS Apple Silicon. It saves them in a draft release for review. There is currently no installer, continuous beta channel, or native in-app updater. Unsigned builds may show a Windows SmartScreen warning. On macOS, run `chmod +x kivo-native-macos-arm64` and `xattr -d com.apple.quarantine kivo-native-macos-arm64` before the first launch.

1. Download the executable for your OS, run it, and open Settings. The published `0.1.0` release contains only the Windows executable. On macOS, use the Homebrew instructions below to build from source.
2. Enter a Gemini API key and choose a model your key can access. You can edit the model ID if the default is retired, without waiting for a new app build. Save settings to keep the model selection.
3. Open Writing tools, paste text, and select an action. Copy the result into your app.

The app keeps your key in memory for the current session, so you need to enter it again after restarting. It saves settings in the OS config directory under `kivo/settings.json`. Text sent to Gemini leaves your computer; requests use `store: false`. The native preview does not yet use OS credential storage (Windows Credential Manager, macOS Keychain) or support the reference backend's other AI providers and local speech models.

The Dictation screen currently accepts typed or pasted text so you can try its UI states. Shortcut, microphone, and startup settings are placeholders until the integrations are ported. The preview supports Windows and macOS Apple Silicon. Linux is used for development and has no supported release.

### Homebrew (macOS Apple Silicon)

The formula builds from a pinned source archive, verifies its checksum, and installs Rust as a build dependency. The first install can take several minutes and requires Apple's Command Line Tools (`xcode-select --install` if they are missing). You do not need a prebuilt macOS release executable.

```sh
brew tap 0libote/kivo https://github.com/0libote/Kivo.git
brew update
brew install 0libote/kivo/kivo
kivo
```

Homebrew requires a tap; raw formula URLs are not supported. The tap command needs the repository URL because this repository is named `Kivo` instead of the expected `homebrew-kivo`. To upgrade to a later release, run `brew update` and `brew upgrade 0libote/kivo/kivo`.

## Develop

Install [rustup](https://rustup.rs/). The repository's `rust-toolchain.toml` selects the Rust version and required components. The native app uses `curl` to communicate with Gemini, so it must be on PATH. Supported Windows and macOS versions include it.

```sh
cargo run --locked --manifest-path native-app/Cargo.toml
```

Run the native checks before submitting a change:

```sh
cargo fmt --manifest-path native-app/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path native-app/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native-app/Cargo.toml
python3 -m unittest discover -s scripts -v
```

`src-tauri/` is the reference backend for integrations still being ported. Its frontend has been removed, so you cannot run it as an alternative app. If you change it, run its format, Clippy, and test checks using the same Cargo commands with `src-tauri/Cargo.toml`. On Ubuntu, these checks require `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, and `libasound2-dev`, plus CMake and a C/C++ compiler. Keep the vendored GLib patch: it fixes an upstream soundness bug and is needed until the reference backend can use a fixed dependency.

CI caches both Rust targets and cancels superseded runs. It checks only the app affected by a change. Documentation-only changes run the release script tests without installing Rust or GTK. A manual CI run checks both apps. The required status is `check`.

## Release

Push a semantic version tag such as `0.2.0` or `app-v0.2.0`. The workflow uses the tag to set the versions in both manifests and lockfiles and the Tauri config, then builds the Windows and macOS releases with locked dependencies. Version syncing uses Python 3.11+ and does not resolve or download dependencies.

The workflow signs the Windows executable if `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD` are configured. macOS builds are unsigned. Review and publish the draft manually, then update `Formula/kivo.rb`: set `version`, pin the source archive `url` to the release commit, and update the archive's `sha256`. The source commit must include macOS support; the original `0.1.0` tag predates it. Re-running a tag updates the existing draft asset. Published releases require a new version tag.

After the build, the workflow opens a version bump PR against `main`, triggers CI, and enables auto-merge. In repository settings, allow Actions to create PRs and enable auto-merge. Older tags cannot downgrade the current native package version.

[Installer artwork](packaging/README.md) remains in the repository for the reference backend. The native release does not use it.
