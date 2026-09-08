# Kivo

Kivo is a small background desktop utility for system-wide dictation and focused writing assistance. It uses native operating-system speech recognition and text-access APIs, with optional Gemini cleanup and rewriting performed from the trusted Rust process.

Kivo has no account system, telemetry, hosted backend, or provider abstraction. Text is sent directly to Google's Gemini API only for an action the user invokes. API keys are stored in macOS Keychain or Windows Credential Manager and are never returned to the webview.

## Supported systems

- macOS 26 or later, distributed directly as a signed and notarized application. The App Sandbox is intentionally disabled because system-wide Accessibility integration is incompatible with it.
- Windows 11 24H2 (build 26100) or later. Native speech recognition requires an installed package identity, so production Windows builds use MSIX.

Physical testing on both systems is required before a release, especially for Fn/Globe handling, Windows-key suppression, speech model availability, multi-monitor placement, accessibility behavior in third-party applications, and signing.

## Prerequisites

### All platforms

1. Install [Bun](https://bun.com/docs/installation).
2. Install stable Rust with [rustup](https://rustup.rs/).
3. Install the current [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

### macOS

- Xcode 26 and its command-line tools.
- A Developer ID Application certificate for distribution and an Apple account configured for notarization.
- Accessibility, Input Monitoring, Microphone, and Speech Recognition permission during development.

### Windows

- Visual Studio Build Tools with Desktop development with C++.
- Windows 11 SDK 10.0.26100 or newer, including MakeAppx and SignTool.
- WebView2 Runtime (included with current Windows 11 installations).
- A code-signing certificate whose subject matches the MSIX publisher.

## Development

```sh
bun install
bun tauri dev
```

The Vite-only preview includes a safe local harness for inspecting all four surfaces without invoking OS integration:

```sh
bun dev
```

Useful checks:

```sh
bun typecheck
bun lint
bun test
bun test:ui
bun run build
bun check:rust
cargo test --manifest-path src-tauri/Cargo.toml
bun tauri build
```

## Gemini setup

Create an API key in [Google AI Studio](https://aistudio.google.com/app/apikey), open Kivo Settings, and save it under AI. Kivo tests the key with Google's current Interactions API.

The native client uses the pinned `gemini-3.8-flash` model with low thinking for latency-sensitive edits. Requests explicitly set `store: false`. Update the model or endpoint only in `src-tauri/src/ai/mod.rs` and update its fixtures at the same time.

Without a key, native dictation still works and inserts the raw operating-system transcript; Gemini-dependent writing actions display a concise configuration error.

## Architecture

The app is one Tauri process with four pre-created webview surfaces:

- `flow-bar`: a non-activating, tightly sized always-on-top dictation overlay.
- `writing-tools`: a compact selection-aware command and result popup.
- `settings`: native-window preferences; closing it hides the window rather than quitting Kivo.
- `onboarding`: a short first-run permission and setup flow.

React owns presentation and transient UI state. Rust owns shortcuts, window placement, speech sessions, selected text, replacements, settings, credentials, Gemini requests, and tray lifecycle. Sensitive text and keys are intentionally absent from serializable types wherever the UI does not need them.

Platform code is isolated under `src-tauri/src/platform/`. macOS 26 uses Accessibility/Core Graphics/AppKit/Keychain and `SpeechAnalyzer` with `DictationTranscriber`. Windows uses UI Automation, Win32 window/input APIs, WinRT speech, and Credential Manager. The platform boundary reserves application-specific and clipboard fallback strategies, but V1 replacement currently fails safely when native Accessibility/UI Automation cannot preserve the original selection.

## Permissions

Kivo asks only in onboarding or when a feature is invoked:

- Accessibility reads the current selection and inserts or replaces text.
- Input Monitoring observes and suppresses the modifier-only dictation gesture.
- Microphone records only while dictation is active.
- Speech Recognition sends audio only to the operating-system speech engine. Microphone audio is never sent to Gemini.

## Packaging and signing

### macOS

`bun tauri build` produces the macOS app and DMG. Configure the standard Tauri Apple signing/notarization environment variables in the release environment. Direct distribution is required; do not enable App Sandbox or submit this build to the Mac App Store.

### Windows MSIX

From a Windows developer shell:

```powershell
.\packaging\windows\build-msix.ps1 -CertificatePath C:\secure\kivo.pfx
```

Replace the placeholder publisher in `packaging/windows/AppxManifest.xml` with the subject of the production certificate or the Microsoft Store identity before signing. The script emits an MSIX under `src-tauri/target/release/bundle/msix`.

### GitHub Releases and updates

Pushing an `app-v*` tag runs `.github/workflows/release.yml`, builds macOS and Windows artifacts, and drafts a GitHub Release. Configure these repository secrets first:

- `TAURI_UPDATER_PUBKEY`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Apple signing/notarization secrets listed in the workflow
- `WINDOWS_CERTIFICATE_BASE64`, `WINDOWS_CERTIFICATE_PASSWORD`, and `WINDOWS_PUBLISHER`

The updater checks `https://github.com/0libote/Kivo/releases/latest/download/latest.json`. Never commit updater private keys or signing certificates.

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. macOS uses Tauri's signed updater metadata; the MSIX build checks the latest GitHub Release and is updated by installing the newer signed package.

## Privacy and diagnostics

Do not add logs containing selected text, transcripts, Gemini responses, clipboard contents, or credentials. User-facing errors are deliberately short; local diagnostics should record only operation names, error categories, and non-sensitive OS codes.
