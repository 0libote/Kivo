# Kivo

Kivo is a small background desktop utility for system-wide dictation and focused writing assistance. It uses native operating-system speech recognition and text-access APIs, with optional Gemini cleanup and rewriting performed from the trusted Rust process.

Kivo has no account system, telemetry, hosted backend, or provider abstraction. Text is sent directly to Google's Gemini API only for an action the user invokes. API keys are stored in macOS Keychain or Windows Credential Manager and are never returned to the webview.

## Supported systems

- macOS 26 or later on Apple Silicon, distributed directly as an application. Stable releases are Developer-ID signed and notarized when the Apple signing secrets are configured; the rolling `continuous` beta is ad-hoc signed (free) and needs a one-time approval in System Settings → Privacy & Security on first launch. The App Sandbox is intentionally disabled because system-wide Accessibility integration is incompatible with it.
- Windows 11 24H2 (build 26100) or later. Distributed as a per-user `.exe` installer. Desktop SAPI speech uses installed Windows speech engines and needs no MSIX identity, Microsoft Store account, or Kivo account.

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
- Windows 11 SDK 10.0.26100 or newer. SignTool is optional for Authenticode signing.
- WebView2 Runtime (included with current Windows 11 installations).
- An installed desktop speech language for dictation. A signing certificate is optional for building and sharing the `.exe`.

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
bun run test
bun test:ui
bun run build
bun check:rust
cargo test --manifest-path src-tauri/Cargo.toml
bun tauri build
```

## Gemini setup

Create an API key in [Google AI Studio](https://aistudio.google.com/app/apikey), open Kivo Settings, and save it under AI. Choose the model under AI → Model, then use Test connection to confirm your key can use it. Kivo tests the key with Google's current Interactions API.

The native client defaults to `gemini-3.8-flash` with low thinking for latency-sensitive edits. Requests explicitly set `store: false`. Any well-formed `gemini-*` text model id works, so newest models keep working without a Kivo update — only speech, image, video, music, and agent families are blocked. Set an optional backup model in Settings → AI: if the primary hits its rate limit, Kivo retries once on the backup before reporting an error. Update the suggestions, blocklist, or endpoint in `src-tauri/src/ai/mod.rs` (mirrored in `src/ai/models.ts`) and update fixtures at the same time.

Without a key, native dictation still works and inserts the raw operating-system transcript; Gemini-dependent writing actions display a concise configuration error.

## Website and YouTube summaries

Use the existing Writing Tools shortcut with webpage text or a YouTube transcript selected, then choose **Summarize**. The result stays in the popup until you copy it or explicitly choose Replace. Text summaries accept up to 200,000 characters; longer content must be split into shorter passages. With nothing selected, choose **Summarize text…** in Quick chat to paste an article or transcript; the normal Ask action still starts quick chat.

Choose **Summarize link…** from either the selection menu or Quick chat to enter a public webpage or YouTube video URL. A selected URL is prefilled. Kivo sends the link to Gemini only when you press Summarize. Webpages use Gemini's URL-context retrieval; YouTube links use its video input. These requests use the existing API key and model, with `store: false`, and do not require another service, a browser extension, or a local downloader. The result shows its source URL and can be copied; link summaries cannot replace the selection, including through the native command. Disabling Summarize in Settings disables both new entry points too.

Link requests have a 90-second deadline; ordinary writing requests retain their 20-second deadline. Closing the popup cancels Kivo's pending request and discards late results. A request already received by Gemini may still consume quota. Public content may be unavailable to Gemini, and URL-context retrieval may use cached content. Private/unlisted YouTube videos and pages requiring sign-in or payment are unsupported. If retrieval fails or the request times out, use **Paste text instead** to supply the article or transcript. Kivo requires successful URL retrieval evidence before displaying a webpage summary.

URLs, supplied text, and summaries are kept in the current writing session, not saved as history or logged. Only the invoked source is sent; Kivo does not read browser tabs, cookies, or browsing history. Gemini's normal API usage limits and billing apply, and processing long videos can use more quota than summarizing pasted text.

For isolated browser verification while another worktree is running:

```sh
KIVO_UI_TEST_PORT=1437 bun run test:ui
```

The browser harness uses illustrative responses and never calls Gemini. Release verification must also exercise a real public article and public video with a configured key, along with native selection, focus, clipboard, and placement checks on macOS and Windows.

## Architecture

The app is one Tauri process with four pre-created webview surfaces:

- `flow-bar`: a non-activating, tightly sized always-on-top dictation overlay.
- `writing-tools`: a compact selection-aware command and result popup.
- `settings`: native-window preferences; closing it hides the window rather than quitting Kivo.
- `onboarding`: a short first-run permission and setup flow.

React owns presentation and transient UI state. Rust owns shortcuts, window placement, speech sessions, selected text, replacements, settings, credentials, Gemini requests, and tray lifecycle. Sensitive text and keys are intentionally absent from serializable types wherever the UI does not need them. The latest completed dictation is kept only in memory for the current app session and can be copied or cleared from Home. Cancelled dictations are discarded.

Platform code is isolated under `src-tauri/src/platform/`. macOS 26 uses Accessibility/Core Graphics/AppKit/Keychain and `SpeechAnalyzer` with `DictationTranscriber`. Windows uses UI Automation, Win32 window/input APIs, desktop SAPI speech, and Credential Manager. Speech uses the system default microphone. Only installed Windows desktop speech languages are offered; recognition quality and language coverage depend on those engines. Native AX/UIA capture leaves the clipboard unchanged. Kivo validates the original field and selection before insertion; unsupported targets or changed selections keep the result available to copy instead of automatically pasting into another field.

## Permissions

Kivo asks only in onboarding or when a feature is invoked:

- Accessibility reads the current selection and inserts or replaces text.
- Input Monitoring observes and suppresses the modifier-only dictation gesture.
- Microphone records only while dictation is active.
- Speech Recognition sends audio only to the operating-system speech engine. Microphone audio is never sent to Gemini.

## Packaging and signing

### macOS

`bun tauri build` produces the macOS app and DMG. The bundle is ad-hoc signed by default (`signingIdentity: "-"` in `src-tauri/tauri.conf.json`, free, no certificate needed) so Gatekeeper shows a recoverable unverified-developer approval instead of the dead-end "damaged" dialog. The stable release workflow overrides this with a real Developer ID via `APPLE_SIGNING_IDENTITY` when the Apple signing/notarization secrets are configured. Direct distribution is required; do not enable App Sandbox or submit this build to the Mac App Store.

Easiest beta install (Apple Silicon, macOS 26+):

```sh
curl -fsSL https://raw.githubusercontent.com/0libote/Kivo/main/scripts/install-macos.sh | bash
```

Manual DMG install: drag `Kivo.app` to `/Applications`, then run once:

```sh
xattr -dr com.apple.quarantine /Applications/Kivo.app
```

Then open Kivo and approve it in System Settings → Privacy & Security if asked. Right-click → Open no longer bypasses Gatekeeper on recent macOS, and the "damaged" wording does not mean the download is corrupt — do not trash the app.

### Windows .exe

```powershell
bun run tauri build --bundles nsis
```

The installer is written to `src-tauri/target/release/bundle/nsis/Kivo_<version>_x64-setup.exe`. It installs for the current user, appears in Start and Installed apps, and enforces Windows 11 24H2 or later. WebView2 is bootstrapped if missing. Host the installer as a public GitHub Release asset: downloading needs no account or payment.

Unsigned builds work but may receive SmartScreen warnings; free hosting does not provide trusted publisher signing. The stable workflow signs the installer when an Authenticode certificate is configured. The older MSIX script remains an optional packaging route and is not used by beta or stable release jobs.

### GitHub Releases and updates

Pushing an `app-v*` tag runs `.github/workflows/release.yml`, builds macOS and Windows artifacts, and drafts a GitHub Release. For signed macOS distribution and updates, configure these repository secrets:

- `TAURI_UPDATER_PUBKEY`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Apple signing/notarization secrets listed in the workflow

Optional Windows signing: `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD`. No Windows signing secret is required to build the installer.

The updater checks `https://github.com/0libote/Kivo/releases/latest/download/latest.json`. Never commit updater private keys or signing certificates.

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. Both desktops check GitHub Releases and hand off to the installer download: stable releases take precedence, and while no stable release exists the rolling `continuous` beta is detected by commit SHA (`continuous.json`, stamped into beta builds via `KIVO_BUILD_SHA`) so same-version rebuilds still show up. An up-to-date stable installation is not offered a rolling beta. Beta builds are ad-hoc signed (free, not notarized) and always require a manual download plus the one-time macOS approval above.

## Privacy and diagnostics

Do not add logs containing selected text, transcripts, Gemini responses, clipboard contents, or credentials. User-facing errors are deliberately short; local diagnostics should record only operation names, error categories, and non-sensitive OS codes.
