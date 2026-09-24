# Kivo

Kivo is a small background desktop utility for system-wide dictation and focused writing assistance. It uses native operating-system speech recognition and text-access APIs, with optional AI cleanup and rewriting performed from the trusted Rust process.

Kivo has no account system, telemetry, or hosted backend. Writing actions send selected text from the Rust process to the provider chosen in Settings: Gemini, OpenCode Zen or Go, or a custom endpoint. Optional dictation cleanup also sends the transcript to that provider when enabled. Local endpoints keep those requests on this computer. API keys are stored in macOS Keychain or Windows Credential Manager and are never returned to the webview.

## Supported systems

- macOS 26 or later on Apple Silicon, distributed directly as an application. Stable releases are Developer-ID signed and notarized when the Apple signing secrets are configured; the rolling `continuous` beta is ad-hoc signed (free) and needs a one-time approval in System Settings → Privacy & Security on first launch. The App Sandbox is intentionally disabled because system-wide Accessibility integration is incompatible with it.
- Windows 11 24H2 (build 26100) or later. Distributed as a per-user `.exe` installer. Desktop SAPI speech uses installed Windows speech engines and needs no MSIX identity, Microsoft Store account, or Kivo account.

Physical testing on both systems is required before a release, especially for Fn/Globe handling, Windows-key suppression, speech model availability, multi-monitor placement, accessibility behavior in third-party applications, and signing.

## Prerequisites

### All platforms

1. Install [Bun](https://bun.com/docs/installation).
2. Install stable Rust with [rustup](https://rustup.rs/).
3. Install the current [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).
4. Install CMake and a C++ toolchain (Visual Studio Build Tools with Desktop development with C++ on Windows, Xcode command-line tools on macOS, `build-essential` on Linux). The on-device speech runtime (`transcribe.cpp`, via the `transcribe-cpp` crate) is compiled from source on the first build; no model files are downloaded at build time.

### macOS

- Xcode 26 and its command-line tools.
- A Developer ID Application certificate for distribution and an Apple account configured for notarization.
- Accessibility, Input Monitoring, Microphone, and Speech Recognition permission during development.

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

The Vite-only preview includes a safe local harness for inspecting all four surfaces without invoking OS integration:

```sh
bun dev
```

Open `http://127.0.0.1:1420/?surface=gallery&harness=1` for the test bench: one screen that probes permissions, microphones, languages, models, key status, dictation start/stop/cancel, a writing smoke test, and a settings write/restore round trip over the active bridge.

### Linux test bench

Linux runs the full app against a simulated adapter, so the shared core (state machines, IPC, settings, AI failover) is exercised exactly as on macOS and Windows. Only macOS and Windows ship; Linux exists so every flow is verifiable without those machines.

```sh
# Tauri system prerequisites for your distro, plus ALSA headers for the
# on-device microphone capture (cpal): libasound2-dev on Debian/Ubuntu, then:
bun install
bun tauri dev
```

Linux behavior: dictation uses a simulated engine (override the transcript with `KIVO_LINUX_DICTATION_TEXT`, the captured text with `KIVO_LINUX_TEST_TEXT`), API keys persist to a dev-only file vault (`~/.config/kivo/linux-credentials.json`, override with `KIVO_LINUX_CREDENTIAL_FILE` in tests — never a shipping credential store), and Copy uses `wl-copy`/`xclip` when present. Dictation holds `Control+Alt+Space`; Writing Tools uses `Ctrl+Space`.

Useful checks:

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
cargo test --manifest-path src-tauri/Cargo.toml
bun tauri build
```

Git hooks are managed by Lefthook: `bun install` installs them, pre-commit runs
Biome + Oxlint on staged files, and pre-push runs the type check and unit tests.

## AI setup

Pick a provider under Settings → AI (or during onboarding), save its key, then arrange your models in order. Switching providers starts a new model queue with that provider's default model. Each provider keeps its own key in macOS Keychain or Windows Credential Manager; keys are never returned to the webview or written to settings files. The model queue lists the models available to your key and shows each model's cost when known, so you can compare before anything is sent. Add up to 5 models, remove them, and reorder with the arrow buttons — requests try the queue top to bottom until one succeeds. A failure moves to the next model; key or balance problems stop immediately without burning further quota. Then use Test connection to verify generation with the first model. Backup entries are checked against the provider model list; their presence does not guarantee generation will succeed.

- **Gemini** (default): create a key in [Google AI Studio](https://aistudio.google.com/app/apikey). The picker lists the text models available to your key via Google's ListModels API (blocklist only — speech/audio, image, video, music, computer-use, and agent families are hidden), falling back to curated suggestions offline or without a key. Test connection sends a short generation request to the first queued model, consuming API quota, then checks backup models against the listing. Slow generation can fall through to the next queued model after the request timeout. Costs are billed by Google.
- **OpenCode Zen**: pay-as-you-go credits from [opencode.ai/auth](https://opencode.ai/auth) at cost (see [Zen pricing](https://opencode.ai/docs/zen#pricing)). The picker pulls the live model list from `https://opencode.ai/zen/v1/models` and prices every entry from a curated table (`src-tauri/src/ai/providers.rs`), so free trial models show `Free` and paid ones show `$X in / $Y out per 1M`. Test connection sends a tiny generation request, so it costs a fraction of a cent.
- **OpenCode Go**: the `$10/month` subscription from [opencode.ai/auth](https://opencode.ai/auth) (see [Go limits](https://opencode.ai/docs/go#usage-limits)). Same live listing as Zen via `https://opencode.ai/zen/go/v1/models`, with each option showing its token rate plus the monthly allowance included in the subscription (e.g. `$0.95 in / $4.00 out per 1M · $60/mo incl.`).
- **Custom (OpenAI-compatible)**: any OpenAI-style endpoint — Ollama (`http://localhost:11434/v1`, the default), LM Studio (`http://127.0.0.1:1234/v1`), llama.cpp, OpenRouter, or any other "normal" OpenCode-style provider from the [provider directory](https://opencode.ai/docs/providers). Enter the base URL, pull a model first (e.g. `ollama pull llama3.1`), then Refresh the model list — models are pulled from your server's `/models` list. No key is needed for local servers; hosted endpoints use the saved key. Local models show `Local · free`. Kivo checks this computer for a running Ollama, LM Studio, or llama.cpp server and offers a one-click **Use** for each one it finds; when none is running it offers to download and launch the official Ollama installer.

Go requests use each model's supported API: Chat Completions for Kimi, Messages for MiniMax/Qwen/Union, and Responses for GPT/Grok/Muse. Kivo identifies itself with its app version and sends a separate OpenCode session header for each stateless request. Sampling parameters use the model defaults so models that reject custom temperature values can work.

The native client defaults to `gemini-3.8-flash` with low thinking for latency-sensitive edits. Requests explicitly set `store: false`. Any well-formed text model id works, so newest models keep working without a Kivo update — only speech/audio, image, video, music, computer-use, and agent families are blocked. Settings files from before the queue store a single `model` plus an optional `backupModel`; they migrate into the queue automatically on first load. To change the Gemini suggestions or blocklist, edit `src-tauri/src/ai/mod.rs` then run `bun run generate:models` (CI fails otherwise); pricing lives in `src-tauri/src/ai/providers.rs` with its offline fallback rows in `src/ai/models.ts`.

Link summaries (webpages via URL context, YouTube via video input) need the Gemini provider. With Zen, Go, or Custom, summarize pasted text instead — the app says so when a link is used there.

Without a key, native dictation still works and inserts the raw operating-system transcript; writing actions with a hosted provider display a concise configuration error.

## On-device transcription

Dictation can run either on the operating-system speech engine or entirely on this computer. Choose the engine under Settings → Dictation → Transcription engine:

- **System** (default) uses the OS engine. On macOS that is Apple's on-device `SpeechAnalyzer`; on Windows it is the installed desktop SAPI engine.
- **On-device** records the default microphone, resamples it to 16 kHz, and transcribes with a downloaded model. Audio never leaves the machine.

On-device models are GGML/GGUF conversions published by [`handy-computer`](https://huggingface.co/handy-computer) on Hugging Face (Apache-2.0 and model-specific licenses), the same files the `transcribe-cpp` runtime is built for. The catalog spans Whisper plus Parakeet, Canary, Moonshine, SenseVoice, Qwen3-ASR, Cohere Transcribe, Nemotron, Granite, Voxtral, GigaAM, and Fun-ASR, so you can trade accuracy, speed, language coverage, and size. Each card shows a family, parameter count, language coverage, and relative accuracy/speed bars. Every model is pinned to a commit and verified by SHA-256 before it is treated as installed, so a moved tag or truncated download can never become a model. Downloads can be cancelled, and installed models deleted, from the same screen.

The runtime is compiled into Kivo, not shipped as a separate server, and links statically on every platform (no extra DLLs to ship): Metal on macOS, Vulkan on Windows x86_64, CPU on Windows-on-ARM and Linux. On Windows the GPU backend is used when a Vulkan-capable driver is present and falls back to CPU otherwise. Building the Windows Vulkan backend needs the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) on the build machine; the SDK installer sets `VULKAN_SDK`, which the build script reads to find `vulkan-1.lib`. End users do not need the SDK.

A pure-Rust voice-activity detector (`earshot`) trims silence around speech before transcription, so a quiet recording reports "no speech" instead of inventing words; it needs no model file and runs identically on every platform.

On-device transcription uses the default microphone unless the selected device can be matched by name; a platform-specific device id that has no matching capture device falls back to the default. Only the latest dictation is kept in memory, as with the system engine.

## Website and YouTube summaries

Use the existing Writing Tools shortcut with webpage text or a YouTube transcript selected, then choose **Summarize**. The result stays in the popup until you copy it or explicitly choose Replace. Text summaries accept up to 200,000 characters; longer content must be split into shorter passages. With nothing selected, the shortcut opens the summarize entry directly so an article or transcript can be pasted, or the source switched to a link.

Choose **Summarize link…** from the selection menu, or switch to **Link** in the no-selection summarize entry, to enter a public webpage or YouTube video URL. A selected URL is prefilled. Kivo sends the link to Gemini only when you press Summarize. Webpages use Gemini's URL-context retrieval; YouTube links use its video input. These requests use the existing API key and model, with `store: false`, and do not require another service, a browser extension, or a local downloader. The result shows its source URL and can be copied; link summaries cannot replace the selection, including through the native command. Disabling Summarize in Settings disables both new entry points too.

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

Presentation uses the Astryx design system (`@astryxdesign/core`) with StyleX for app-specific layout. The Kivo theme is defined in `src/theme/kivo.ts` and pre-compiled to `src/theme/built/` by `bun run theme:build`; both the built CSS and JS are committed and checked in CI by `bun run theme:check`. Motion (`motion/react`) drives the compact overlay and step transitions. There is no hand-written component CSS: document-level rules live in `src/styles/app.css` and the only other stylesheets are Astryx's prebuilt CSS and the markdown prose rules for sanitized AI output.

Platform code is isolated under `src-tauri/src/platform/`. macOS 26 uses Accessibility/Core Graphics/AppKit/Keychain and `SpeechAnalyzer` with `DictationTranscriber`. Windows uses UI Automation, Win32 window/input APIs, desktop SAPI speech, and Credential Manager. The optional on-device engine lives under `src-tauri/src/speech/` (`local.rs` capture and inference, `model_store.rs` catalog/downloads, `router.rs` per-session engine selection) and is shared by both desktops. Speech uses the system default microphone. Only installed Windows desktop speech languages are offered; recognition quality and language coverage depend on those engines. Kivo prefers clipboard-free AX/UIA capture, then uses a guarded Copy/Paste transaction for editors that do not expose usable text accessibility. The transaction snapshots every clipboard representation and restores it only while Kivo still owns the clipboard. Kivo validates the original application, field, and available selection/caret identity before insertion; changed targets keep the result available to copy instead of automatically pasting into another field.

## Permissions

Kivo asks only in onboarding or when a feature is invoked:

- Accessibility reads the current selection and inserts or replaces text.
- Input Monitoring observes and suppresses the modifier-only dictation gesture.
- Microphone records only while dictation is active.
- Speech Recognition sends audio only to the operating-system speech engine. With the on-device engine, microphone audio is transcribed locally and never sent off the device. Microphone audio is never sent to Gemini.

On Windows there is no in-app consent prompt: the microphone row reads
granted (capture problems surface when dictation starts, pointing back at
the microphone privacy settings), and Speech Recognition reflects the
installed desktop speech languages — an empty engine list shows guidance
to install one instead of an Allow button that could never resolve.

### macOS beta: repeated prompts and "Settings shows on, app shows off"

The rolling `continuous` beta is ad-hoc signed (`signingIdentity: "-"`,
free). macOS TCC keys Accessibility and Input Monitoring grants to the
code signature, not just the bundle id, so every rebuilt/updated beta
looks like a brand-new app: System Settings may still list Kivo as
enabled while `AXIsProcessTrusted()` returns false, unlocking Privacy &
Security asks for a password each time, and Keychain may re-prompt for
the API-key item after an update. This is expected for ad-hoc builds —
stable `app-v*` releases are Developer-ID signed and notarized when the
Apple secrets are configured, and their grants persist across updates.

What to check on the Mac:

- `codesign -dv --verbose=4 /Applications/Kivo.app` — `Signature=adhoc`
  means grants will not survive updates; a Developer ID line means they should.
- If an old build's entry is stuck and the new one can't be enabled, use
  Settings → Permissions → Clear stale entries (runs
  `tccutil reset All com.kivo.desktop` for Kivo only, no sudo needed),
  then re-allow each permission in turn. Manual equivalent:
  `tccutil reset All com.kivo.desktop`, then re-add Kivo in
  System Settings → Privacy & Security → Accessibility.
- `log show --last 10m --predicate 'process == "Kivo"'` and Console.app
  show the prompt / TCC denial lines; Kivo never logs text, transcripts,
  or keys.
- In-app, Settings → Permissions now refreshes automatically (poll +
  window focus) after you grant in System Settings; the first Allow click
  shows the system prompt, a still-off state afterwards means open System
  Settings and toggle Kivo there.

## Packaging and signing

### macOS

`bun tauri build` produces the macOS app and DMG. The bundle is ad-hoc signed by default (`signingIdentity: "-"` in `src-tauri/tauri.conf.json`, free, no certificate needed) so Gatekeeper shows a recoverable unverified-developer approval instead of the dead-end "damaged" dialog. The stable release workflow overrides this with a real Developer ID via `APPLE_SIGNING_IDENTITY` when the Apple signing/notarization secrets are configured. Direct distribution is required; do not enable App Sandbox or submit this build to the Mac App Store.

Easiest beta install (Apple Silicon, macOS 26+):

```sh
curl -fsSL --proto '=https' --tlsv1.2 https://raw.githubusercontent.com/0libote/Kivo/main/scripts/install-macos.sh | bash
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

Unsigned builds work but may receive SmartScreen warnings; free hosting does not provide trusted publisher signing. The stable workflow signs the installer when an Authenticode certificate is configured. NSIS is the only Windows packaging route.

### GitHub Releases and updates

Pushing an `app-v*` tag runs `.github/workflows/release.yml`, builds macOS and Windows artifacts, and drafts a GitHub Release. Both stable and rolling beta in-app updates use these repository secrets:

- `TAURI_UPDATER_PUBKEY`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- Apple signing/notarization secrets listed in the workflow

Optional Windows signing: `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_CERTIFICATE_PASSWORD`. No Windows signing secret is required to build the installer.

The updater checks `https://github.com/0libote/Kivo/releases/latest/download/latest.json`. Never commit updater private keys or signing certificates.

`bun run prepare:release` creates the ignored release-only Tauri config and injects the updater public key from the environment. Normal local builds intentionally have no trusted updater key and can check availability but cannot install a signed update. Both desktops check GitHub Releases: a newer stable release takes precedence; beta installations also follow the rolling `continuous` release when stable is already current. The beta manifest identifies same-version rebuilds by commit SHA and carries signed updater archives for both platforms. An up-to-date stable installation is not offered a rolling beta. Settings → About offers **Download and Install** for available stable or beta updates, then restarts automatically to finish, with a **Restart now** fallback if the app remains open. Existing beta installations without an embedded updater public key require one manual upgrade to a signed beta build before in-app updates can work. Beta builds remain ad-hoc signed on macOS (free, not notarized), so first installation may require the one-time macOS approval above.

## Privacy and diagnostics

Do not add logs containing selected text, transcripts, Gemini responses, clipboard contents, or credentials. User-facing errors are deliberately short; local diagnostics should record only operation names, error categories, and non-sensitive OS codes.
