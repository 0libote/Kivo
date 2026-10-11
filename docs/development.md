# Development

[Back to Kivo](../README.md)

## Prerequisites

### All platforms

1. Install [Bun](https://bun.com/docs/installation).
2. Install stable Rust with [rustup](https://rustup.rs/).
3. Install the current [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).
4. Install CMake and a C++ toolchain (Visual Studio Build Tools with Desktop development with C++ on Windows, `build-essential` on Linux). The on-device speech runtime (`transcribe.cpp`, via the `transcribe-cpp` crate) is compiled from source on the first build; no model files are downloaded at build time.

### Windows

- Visual Studio Build Tools with Desktop development with C++.
- Windows 11 SDK 10.0.26100 or newer. SignTool is optional for Authenticode signing.
- WebView2 Runtime (included with current Windows 11 installations).
- The [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) for the on-device speech runtime's GPU backend (x64 only). Install it once; it sets `VULKAN_SDK` for new terminals.
- An installed desktop speech language for dictation. A signing certificate is optional for building and sharing the `.exe`.

If more than one Visual Studio install is present (for example Build Tools 2026 plus VS 2022), CMake may default to one without the C++ workload. Point it at a complete install for the build shell, e.g. `$env:CMAKE_GENERATOR = "Visual Studio 17 2022"` in PowerShell before `bun tauri dev`.

## Development

```sh
bun install
bun tauri dev
```

For a local installer/package, prefer the fast release-like build:

```sh
bun run desktop:build
```

That keeps Cargo output in the normal release directory but uses thin LTO,
16 codegen units, and normal speed-oriented optimisation instead of the stable
release's fat-LTO/single-codegen link. If `sccache` is already on `PATH`, the
script also uses it for Rust and C/C++ compilation. The tagged release workflow
still uses the full `[profile.release]` settings. Use `bun tauri build`
directly only when you specifically want to reproduce that slower stable
release profile. Pass normal Tauri build arguments after `--`, for example
`bun run desktop:build -- --bundles nsis`.

The Vite-only preview includes a safe local harness for inspecting all four surfaces without invoking OS integration:

```sh
bun dev
```

Open `http://127.0.0.1:1420/?surface=gallery&harness=1` for the test bench: one screen that probes permissions, microphones, languages, models, key status, dictation start/stop/cancel, a writing smoke test, and a settings write/restore round trip over the active bridge.

### Linux test bench

Linux runs the full app against a simulated adapter, so the shared core (state machines, IPC, settings, AI failover) is exercised through the same application contracts as Windows. Windows is the sole supported product; Linux is a development bench with no desktop feature-parity promise. Browser tests and shared Rust tests run on Linux, but Windows UI Automation, hooks, WebView2, audio devices, and installers require Windows verification.

```sh
# Tauri system prerequisites for your distro, plus ALSA headers for the
# on-device microphone capture (cpal): libasound2-dev on Debian/Ubuntu, then:
bun install
bun tauri dev
```

Linux behavior: dictation uses a simulated engine (override the transcript with `KIVO_LINUX_DICTATION_TEXT`, the captured text with `KIVO_LINUX_TEST_TEXT`), API keys persist to a dev-only file vault (`~/.config/kivo/linux-credentials.json`, override with `KIVO_LINUX_CREDENTIAL_FILE` in tests — never a shipping credential store), and Copy uses `wl-copy`/`xclip` when present. Dictation holds `Control+Alt+Space`; Writing Tools uses `Ctrl+Space`.

Quick checks (use `bun run ai-check:full` for the authoritative application, Rust, full browser, built-frontend, bundle, and website gate):

```sh
bun typecheck
bun lint              # Oxlint
bun run format:check  # Biome (use `bun run format` to apply)
bun run knip          # dead files, exports and dependencies
bun run theme:check   # Astryx theme artifacts are current
bun run test          # bun test
bun test:ui           # Playwright
bun run build
bun check:rust
cargo test --locked --manifest-path src-tauri/Cargo.toml
bun run desktop:build  # fast local package
bun tauri build        # full stable release profile
```

Git hooks are managed by Lefthook: `bun install` installs them, pre-commit runs
Biome + Oxlint on staged files, and pre-push runs the type check and unit tests.

## Architecture

The app is one Tauri process with four pre-created webview surfaces:

- `flow-bar`: a non-activating, tightly sized always-on-top dictation overlay.
- `writing-tools`: a compact selection-aware command and result popup.
- `settings`: native-window preferences; closing it hides the window rather than quitting Kivo.
- `onboarding`: a short first-run permission and setup flow.

React owns presentation and transient UI state. Rust owns shortcuts, window placement, speech sessions, selected text, replacements, settings, credentials, Gemini requests, and tray lifecycle. Sensitive text and keys are intentionally absent from serializable types wherever the UI does not need them. The latest completed dictation is kept only in memory for the current app session and can be copied or cleared from Home. Cancelled dictations are discarded.

Presentation uses the Astryx design system (`@astryxdesign/core`) with StyleX for app-specific layout. The Kivo theme is defined in `src/theme/kivo.ts` and pre-compiled to `src/theme/built/` by `bun run theme:build`; both the built CSS and JS are committed and checked in CI by `bun run theme:check`. Motion (`motion/react`) drives the compact overlay and step transitions. There is no hand-written component CSS: document-level rules live in `src/styles/app.css` and the only other stylesheets are Astryx's prebuilt CSS and the markdown prose rules for sanitized AI output.

The Bun patch for `@astryxdesign/cli@0.6.3` prevents name-only component references from erasing documented built-in variants. This keeps theme declarations independent of filesystem traversal order. `tests/theme-generation.test.ts` checks both scan orders; remove the patch when the upstream generator fixes this behavior.

Platform code is isolated under `src-tauri/src/platform/`. Windows uses UI Automation, Win32 window/input APIs, desktop SAPI speech, and Credential Manager. Shared speech routing, capture, and session handling live under `src-tauri/src/speech/`; `local.rs` and `model_store.rs` retain Kivo's native transcribe-cpp engine. Linux continues to use the simulated system speech adapter. Speech uses the system default microphone. Only installed Windows desktop speech languages are offered; recognition quality and language coverage depend on those engines. Kivo prefers clipboard-free UIA capture, then uses a guarded Copy/Paste transaction for editors that do not expose usable text accessibility. The transaction snapshots every clipboard representation and restores it only while Kivo still owns the clipboard. Kivo validates the original application, field, and available selection/caret identity before insertion; changed targets keep the result available to copy instead of automatically pasting into another field.

## Support and verification policy

Windows x86_64 on Windows 11 24H2+ is the release target. macOS desktop support has been removed; browser-only frontend development remains possible on other hosts. Linux is retained solely for the simulated native bench and shared tests, not distribution. Do not add cross-desktop parity requirements.

CI uses fail-safe change classification: application changes build and exercise a Windows review NSIS installer; native/shared/build configuration changes additionally run Windows and Linux Clippy/tests. Documentation-only changes keep workflow validation and the required aggregate; website changes run website checks. Main and release verification run the complete shared gate. Linux runs shared Rust and simulated adapter tests plus the browser harness. The browser suite exercises Windows behavior with a Windows user agent on either host; it does not execute Win32 or WebView2 APIs. Before release, follow the native Windows checklist in the release guide.

## Generated contracts and speech engines

Run `bun run generate:models` after changing the Gemini catalogue/blocklist and `bun run generate:contracts` after changing Rust provider/catalogue/default-setting serialization. Cargo tests reject stale native snapshots; TypeScript checks their public shapes. This caught pricing-field naming mismatches that source-name checks could not catch.

The experimental Voz/Ear browser integration was removed: its license requires SDK usage telemetry, incompatible with Kivo's no-telemetry promise. System speech and native downloaded models remain. Legacy persisted `voz` engine preferences migrate to System; the flow bar clears only known retired browser model caches. The shipping CSP permits no remote scripts, browser workers, or ML CDNs.

Browser tests own ports 1427 (dev) and 1428 (production), refuse server reuse, and wait for rendered/settings-ready surfaces. `bun run test:ui:production` builds a separate `dist-browser` Windows harness with an explicit mock opt-in. Normal desktop builds exclude that harness. See [repository health](repository-health-plan.md) for baseline measurements, remediation, retained dependencies, and maintenance ownership.
