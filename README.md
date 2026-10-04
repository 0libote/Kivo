# Kivo

System-wide dictation and writing tools for macOS and Windows. Dictate into your current app, rewrite selected text, or summarize text and links from a compact popup.

Kivo runs in the background. It has no account system, telemetry, or hosted analytics.

## Download

Get an installer from [GitHub Releases](https://github.com/0libote/Kivo/releases). The [latest beta](https://github.com/0libote/Kivo/releases/tag/continuous) follows `main` and may contain unfinished changes.

| Platform | Requirements | Download |
| --- | --- | --- |
| macOS | Apple Silicon, macOS 26+ | `.dmg` |
| Windows | Windows 11 24H2+ | `-setup.exe` |

The beta is not a stable release. The project has no Apple Developer Program subscription, so macOS beta builds are ad-hoc signed and not notarized: Gatekeeper may block the first launch until explicitly allowed, and Accessibility / Input Monitoring grants may need to be renewed after every update. Windows builds may show a SmartScreen warning. See [installation and signing](docs/releasing.md) for details.

Linux is a development test bench with simulated OS integration; it is not a supported release platform.

## Use Kivo

1. Open Kivo and complete the permission setup.
2. Use the configured dictation shortcut to speak into the current text field.
3. Select text and use the Writing Tools shortcut to proofread, rewrite, or summarize it.
4. For AI features, choose a provider or local endpoint in Settings → AI.

System dictation works without an AI key. Optional on-device transcription models can be installed in Settings → Dictation. Hosted writing tools and optional dictation cleanup send text to the provider you choose; local AI endpoints keep those requests on your computer. Link summaries require Gemini.

Keys are stored in macOS Keychain or Windows Credential Manager. Kivo keeps the latest dictation in memory for recovery, rather than saving a transcript history.

## Develop

Install Bun, stable Rust, and the [platform build prerequisites](docs/development.md#prerequisites), then:

```sh
bun install
bun tauri dev
```

For a browser preview without OS integration:

```sh
bun dev
```

Open `http://127.0.0.1:1420/?surface=gallery&harness=1` to inspect the test bench. For a local desktop package, run `bun run desktop:build`.

Run the checks before submitting a change:

```sh
bun run check
bun run test:ui
bun run build
bun run check:rust
bun run lint:rust
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [User guide](docs/user-guide.md): AI setup, speech engines, summaries, permissions, and troubleshooting.
- [Development](docs/development.md): toolchains, browser harness, Linux bench, and architecture.
- [Releases](docs/releasing.md): installers, signing, release channels, and in-app updates.
- [Website](website/README.md): preview and deploy the static site.
- [Repository audit](docs/repo-audit.md): cleanup findings and remaining release checks.
