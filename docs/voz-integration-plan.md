> Archived experiment: removed in PR #78 because the SDK license requires usage telemetry, conflicting with Kivo's no-telemetry product promise. System speech and native on-device models remain. This document is historical, not an implementation plan.

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

## Runtime choices

```text
Windows   → official @desert-ant-labs/voz + @desert-ant-labs/ear 3.5.0 in a lazy, dedicated Web Worker; ONNX Runtime Web / WebGPU
Linux     → unsupported Voz capability; existing simulated speech adapter
UI tests  → mocked bridge; never download model weights
```

The browser SDK requires
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
the pinned SDK does not provide a Windows native product. The dedicated
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
- [x] Add the Windows worker adapter.
- [x] Integrate model lifecycle, settings, and onboarding preparation.
- [x] Add mocked tests and manual hardware integration guidance.
- [x] Update packaging, platform parity, and user/contributor documentation.
- [x] Run the locally available repository gates: `bun run check`, `bun test:ui`,
  `bun run build`, Rust clippy/tests, the Linux Tauri debug build, and the
  Windows-target Vite asset build.
- [ ] Build/sign the Windows Tauri release package and run hardware transcription.

## Contributor smoke test

Normal CI and browser tests mock the runtime; they must not download model
weights. To verify real inference on supported hardware:

1. On supported Windows 11, open Settings → Dictation.
2. Download the optional Voz model and wait for preparation to finish. Check
   that progress appears without inventing a percentage when the runtime has no
   progress value.
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

Windows Voz and Ear SDKs and their pinned WebAssembly runtimes are downloaded
on demand in the flow-bar worker. Models use the WebView cache. Vite substitutes
a tiny worker stub in Linux builds. Verify inference and the SDK CPU fallback
in the packaged x64 NSIS installer; Linux cannot certify WebView2 or GPU drivers.
