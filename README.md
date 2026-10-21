# Kivo

Kivo is a Windows dictation and writing utility being ported from Tauri to a native Rust UI.
The current native build is a development preview: writing tools accept pasted text and call Gemini, while dictation, global shortcuts, system selection capture, and text replacement still need their OS integrations ported.

## Download and use

Published builds appear on [GitHub Releases](https://github.com/0libote/Kivo/releases).
The release workflow produces a portable `kivo-native.exe` for Windows 11 24H2+ and leaves it in a draft for review. There is currently no installer, continuous beta channel, or native in-app updater. Unsigned builds may show a Windows SmartScreen warning.

1. Run `kivo-native.exe` and open Settings.
2. Enter a Gemini API key and choose a model available to your key. The model ID is editable so a retired default does not require a new app build. Save settings to keep the model selection.
3. Open Writing tools, paste text, and select an action. Copy the result into your app.

The key stays in memory for the current session; enter it again after restarting. Settings are saved in the OS config directory under `kivo/settings.json`. Text sent to Gemini leaves your computer; requests use `store: false`. The native preview does not yet use Windows Credential Manager or support the reference backend's other AI providers and local speech models.

The Dictation surface currently uses typed or pasted text to exercise its UI states. Shortcut, microphone, and startup settings are placeholders until the integrations are ported. Linux is a development bench, not a supported release target.

## Develop

Install [rustup](https://rustup.rs/). The repository's `rust-toolchain.toml` selects Rust and the required components. The native Gemini transport also requires `curl` on PATH (included with supported Windows versions).

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

`src-tauri/` is the reference backend for integrations still being ported. Its frontend has been removed, so it is not a runnable alternative app. Changes there also need its format, Clippy, and test checks; use the same Cargo commands with `src-tauri/Cargo.toml`. On Ubuntu these require `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, and `libasound2-dev`, plus CMake and a C/C++ compiler. The vendored GLib patch fixes an upstream soundness bug and must stay until the reference backend can use a fixed dependency.

CI caches both Rust targets, cancels superseded runs, and checks only the app affected by a change. Documentation-only changes run the release script tests without installing Rust or GTK. A manual CI run checks both apps. The required status remains `check`.

## Release

Push a semantic version tag such as `0.2.0` or `app-v0.2.0`. The tag sets the versions in both manifests and lockfiles and the Tauri config before a locked Windows release build. Version syncing uses Python 3.11+ and does not resolve or download dependencies.

If `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD` are configured, the executable is signed. Review and publish the resulting draft manually. Re-running a tag updates the existing draft asset; published releases require a new version tag.

After the build, the workflow opens a version bump PR against `main`, explicitly dispatches CI, and enables auto-merge. This requires Actions to be allowed to create PRs and auto-merge to be enabled in repository settings. Older tags cannot downgrade the current native package version.

[Installer artwork](packaging/README.md) is retained for the reference backend; it is not part of the native release.
