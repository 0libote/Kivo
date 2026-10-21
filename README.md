# Kivo

System-wide dictation and writing tools for Windows. Dictate into your current app, rewrite selected text, or summarize text and links from a compact popup.

Kivo runs in the background. It has no account system or Kivo-hosted analytics. Optional Desert Ant Voz/Ear models send required device-usage telemetry to Desert Ant Labs; audio and transcripts stay on-device. Review and acknowledge this in Settings before using them. See [data collection](docs/user-guide.md#data-collection).

## Download

Get an installer from [GitHub Releases](https://github.com/0libote/Kivo/releases). The [latest beta](https://github.com/0libote/Kivo/releases/tag/continuous) follows `main` and may contain unfinished changes.

| Platform | Requirements | Download |
| --- | --- | --- |
| Windows | Windows 11 24H2+ | `-setup.exe` |

The beta is not a stable release. Windows builds may show a SmartScreen warning. See [installation and signing](docs/releasing.md) for details.

Linux is a development test bench with simulated OS integration; it is not a supported release platform.

## Use Kivo

1. Open Kivo and complete the permission setup.
2. Use the configured dictation shortcut to speak into the current text field.
3. Select text and use the Writing Tools shortcut to proofread, rewrite, or summarize it.
4. For AI features, choose a provider or local endpoint in Settings → AI.

System dictation works without an AI key. Optional on-device transcription models can be installed in Settings → Dictation. Hosted writing tools and optional dictation cleanup send text to the provider you choose; local AI endpoints keep those requests on your computer. Link summaries require Gemini.

Keys are stored in Windows Credential Manager. Kivo keeps the latest dictation in memory for recovery, rather than saving a transcript history.

## Develop

Install stable Rust with [rustup](https://rustup.rs/), then:

```sh
cargo run --manifest-path native-app/Cargo.toml
```

Run the checks before submitting a change:

```sh
cargo fmt --manifest-path native-app/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path native-app/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native-app/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

`src-tauri/` remains as the reference backend for the OS integrations being
ported to the native app. The `docs/` guides still describe the previous
Tauri layout and are pending a rewrite.

## Documentation

- [User guide](docs/user-guide.md): AI setup, speech engines, summaries, permissions, and troubleshooting.
- [Development](docs/development.md): toolchains, browser harness, Linux bench, and architecture.
- [Releases](docs/releasing.md): installers, signing, release channels, and in-app updates.
- [Website](website/README.md): preview and deploy the static site.
- [Repository audit](docs/repo-audit.md): cleanup findings and remaining release checks.
