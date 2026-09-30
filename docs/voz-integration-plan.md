# Voz integration plan

## Verified upstream contract

Checked against Desert Ant Labs' `desert-ant-core` README, `docs/models/voz.md`,
model page, `llms.txt`, the `v3.5.0` source tag, and the JavaScript package README
on 2026-09-29. The current SDK/package version is 3.5.0. Voz is a batch
recognizer: it accepts mono samples and returns final text, word start/end times,
duration, processing time, and download progress. There is no public streaming
API. Its API does not accept a language argument and the model does not identify
the language. The official Voz guide recommends pairing it with Ear, checking
`isReliable`, and verifying the result against `Voz.supportedLanguages`. Kivo
therefore includes Desert Ant's Ear model as an auxiliary local language check
and compares reliable detections with the selected language. `auto` accepts only
reliable supported detections. Uncertain, mismatched, or unsupported audio fails
with guidance; Kivo does not silently route it to another engine.

Voz's SDK has usage reporting enabled by default on Apple. Kivo must set the
documented `DesertAnt.usageDisabled` switch before constructing Voz so its
no-telemetry privacy model remains intact. Audio and transcript content are not
sent by Voz inference.

## Runtime choices

```text
macOS     → Swift Voz + Ear 3.5.0 in a small C-ABI bridge called by Rust; Core ML / Neural Engine
Windows   → official @desert-ant-labs/voz + @desert-ant-labs/ear 3.5.0 in a lazy, dedicated Web Worker; ONNX Runtime Web / WebGPU
Linux     → unsupported Voz capability; existing simulated speech adapter
UI tests  → mocked bridge; never download model weights
```

The macOS packages are the native fit for Kivo's Apple Silicon minimum and use
Core ML without moving microphone ownership out of Rust. The Swift bridge accepts
captured mono samples, uses Ear to check language, and returns Voz's result to
Rust, where text and timestamps remain out of frontend state except for text
that the existing dictation UI needs. It disables upstream usage reporting.

The stable 3.5.0 Swift package source is Apple-only; Windows support is exposed
by the official JavaScript/browser SDK in that release. The browser SDK requires
WebGPU and can fall back to CPU if the adapter cannot compile its f16 shaders.
The Voz browser bundle is about 390 MB and resident memory about 1.2 GB. Windows uses a
dedicated lazy worker so the SDK/runtime are not part of startup and normal UI
does not handle microphone access. Rust continues to own capture, VAD, session
identity, cancellation, routing, and result delivery. Audio samples cross the
private Rust-to-worker bridge only for local inference; they are never persisted
or sent to a service. Model files use the SDK's Cache API. Browser UI tests mock
the worker.

The Windows adapter waits for the flow-bar WebView to register its request
listener and for the worker module to report that it loaded before dispatching
status, download, or transcription requests. This avoids losing the first
request while the hidden flow-bar window starts. A worker startup failure is
reported as a runtime error; Settings does not mislabel it as an unsupported
platform.

The SDK browser runtime cannot currently be initialized from the Rust side, and
the tagged Swift package does not provide a Windows native product. The dedicated
worker is therefore the supported Windows route in 3.5.0, despite its higher
memory cost; it avoids shipping an untagged native build or adding a Node runtime
to Kivo. This choice should be revisited when Desert Ant publishes a tagged
Windows native package with its cache/download contract.

Ear is included because the upstream Voz API has no language-selection
parameter. This is a deliberate addition: the official SDK docs recommend Ear
for routing, and Kivo must not imply that a selected language controls a model
that cannot receive it. Ear's model is also downloaded and cached during setup.

## Languages

Voz's supported ISO 639-1 codes are `bg`, `cs`, `da`, `de`, `el`, `en`, `es`,
`et`, `fi`, `fr`, `hr`, `hu`, `it`, `lt`, `lv`, `mt`, `nl`, `pl`, `pt`, `ro`,
`ru`, `sk`, `sl`, `sv`, and `uk`. Ear must return a reliable match from this set.
If a specific language is selected, Kivo also requires Ear's match to agree. The
`auto` setting uses Ear's on-device match. A mismatch, unsupported language, or
uncertain Ear result is reported as an error; the user can select System or
Kivo's existing local engine explicitly.

## Stages

- [x] Inspect Kivo speech, platform, bridge, settings, test, and release paths.
- [x] Verify current upstream docs, version, API, languages, and runtime limits.
- [x] Record platform/runtime architecture before implementation.
- [x] Add the shared Voz backend, result metadata, and session-safe routing.
- [x] Add the native macOS bridge and Windows worker adapter.
- [x] Integrate model lifecycle, settings, and onboarding preparation.
- [x] Add mocked tests and manual hardware integration guidance.
- [x] Update packaging, platform parity, and user/contributor documentation.
- [x] Run the locally available repository gates: `bun run check`, `bun test:ui`,
  `bun run build`, Rust clippy/tests, the Linux Tauri debug build, and the
  Windows-target Vite asset build.
- [ ] Build/sign the actual macOS and Windows Tauri release packages and run
  hardware transcription. This Linux host has neither Apple Clang/SwiftPM nor
  MSVC `lib.exe`; the cross-target Rust checks stop at those missing native
  toolchains, so use the existing macOS and Windows CI builders before shipping.

## Contributor smoke test

Normal CI and browser tests mock the runtime; they must not download model
weights. To verify real inference on supported hardware:

1. On Apple Silicon macOS or supported Windows 11, open Settings → Dictation.
2. Download the optional Voz model and wait for preparation to finish. Check
   that progress appears without inventing a percentage when the runtime has no
   progress value. On macOS, allow the first Core ML preparation to complete.
3. Select Voz and a supported language, then dictate a short, clear sentence.
   Confirm Kivo inserts the final text only after capture stops and that no
   partial transcript is shown while recording.
4. Set language to Auto and test a clear supported language. Then select a
   different language than the one spoken and confirm Kivo reports the mismatch
   instead of silently switching engines.
5. Start a dictation and cancel it before inference completes. Confirm the
   result is discarded and cannot appear in a later dictation session.
6. After downloading, disconnect networking and repeat the transcription to
   confirm inference remains on-device. The initial model download requires
   network access.
7. Remove Voz from settings and confirm Kivo returns to the not-downloaded
   state. The browser Ear SDK does not expose removal of its HTTP cached assets;
   clearing Voz removes its SDK Cache API entries and Kivo's readiness marker.

Keep any audio fixture short, consented, and local. Compare the transcript
semantically rather than requiring exact punctuation because model revisions
may change formatting. Do not check model files into this repository.

## Distribution notes

The macOS VozBridge Swift package resolves Desert Ant's pinned 3.5.0 products
at build time and embeds the generated dynamic library in the app Frameworks
directory. Release CI must sign/notarize this dylib with the containing app and
retain the existing hardened-runtime entitlements. The local Linux environment
cannot verify Apple's linker, code signing, notarization, or Neural Engine
preparation; run the release workflow on Apple Silicon before shipping.

Windows' Voz and Ear JavaScript SDKs are loaded only in the flow-bar worker.
The Windows build emits the SDK WebAssembly runtimes as worker assets (roughly
119 MB combined); the speech models themselves are downloaded to the WebView
cache on demand and are not embedded in the installer. Vite substitutes a tiny
worker stub in macOS/Linux builds, so those bundles do not carry the Windows
runtime. WebView2 must
support the SDK's WebGPU path, with the SDK's CPU fallback where supported.
Check the packaged x64 NSIS installer on a clean supported Windows machine;
the Linux development environment cannot validate WebView2, DirectML/WebGPU
drivers, or Windows installer behavior.
