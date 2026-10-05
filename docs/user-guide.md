# Using Kivo

[Back to Kivo](../README.md)

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
- **Voz · Recommended** is an optional add-on for Apple Silicon macOS and Windows. Install it from Settings → Dictation before selecting it. Kivo downloads Voz (about 467 MB on macOS or 390 MB on Windows) and the Ear spoken-language checker as a separate model; it does not add either model to the app installer. On Windows, a lazy Web Worker downloads the pinned SDK and ONNX/WebAssembly runtimes on demand; those runtimes are not bundled in the installer. Windows may use about 1.2 GB of memory while Voz is loaded. The first macOS preparation can take about 20 seconds.

Voz returns a final transcript after recording stops; it does not provide live partial text. Kivo keeps its word timestamps and checks the audio language locally with Ear before transcription. Automatic accepts only a reliable match among Voz's 25 languages; a selected language must match the detected language. Uncertain, unsupported, or mismatched audio produces an error so you can choose another engine. Kivo never changes to System or another engine without your choice. The model downloads during setup, so first dictation does not initiate a large download or Core ML preparation.

On-device models are GGML/GGUF conversions published by [`handy-computer`](https://huggingface.co/handy-computer) on Hugging Face (Apache-2.0 and model-specific licenses), the same files the `transcribe-cpp` runtime is built for. The catalog spans Whisper plus Parakeet, Canary, Moonshine, SenseVoice, Qwen3-ASR, Cohere Transcribe, Nemotron, Granite, Voxtral, GigaAM, and Fun-ASR, so you can trade accuracy, speed, language coverage, and size. Each card shows a family, parameter count, language coverage, and relative accuracy/speed bars. Every model is pinned to a commit and verified by SHA-256 before it is treated as installed, so a moved tag or truncated download can never become a model. Downloads can be cancelled, and installed models deleted, from the same screen.

Kivo's existing local runtime is compiled into Kivo, not shipped as a separate server, and links statically on every platform (no extra DLLs to ship): Metal on macOS, Vulkan on Windows x86_64, CPU on Windows-on-ARM and Linux. On Windows the GPU backend is used when a Vulkan-capable driver is present and falls back to CPU otherwise. Building the Windows Vulkan backend needs the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) on the build machine; the SDK installer sets `VULKAN_SDK`, which the build script reads to find `vulkan-1.lib`. End users do not need the SDK. Voz is a separate engine and uses the platform runtimes described above.

A pure-Rust voice-activity detector (`earshot`) trims silence around speech before transcription, so a quiet recording reports "no speech" instead of inventing words; it needs no model file and runs identically on every platform.

On-device transcription uses the default microphone unless the selected device can be matched by name; a platform-specific device id that has no matching capture device falls back to the default. Only the latest dictation is kept in memory, as with the system engine.

## Custom words

Names, acronyms, and terms that dictation keeps getting wrong (people, products, jargon) can be taught under Settings → Dictation → Custom words. Add the exact spelling you want, plus a short note on what it means — for example `SOC 2` with `compliance framework`, or a client's name with how to use it. Words take effect on the next dictation.

Custom words work on every transcription engine: they are passed as a hint to Whisper-family on-device models and carried as protected terms into AI dictation cleanup, so cleanup will not "correct" them into everyday words. Writing Tools proofreading and rewriting protects them too. The list holds up to 200 words and works without an AI key; import and export move it between machines as plain text (`word | meaning` per line). Keep it focused on the terms you actually mishear — a short list is a stronger hint than a long one.

## Website and YouTube summaries

Use the existing Writing Tools shortcut with webpage text or a YouTube transcript selected, then choose **Summarize**. The result stays in the popup until you copy it or explicitly choose Replace. Text summaries accept up to 200,000 characters; longer content must be split into shorter passages. With nothing selected, Kivo asks you to select text first.

Select a public webpage or YouTube URL, then invoke Writing Tools. If Summarize is enabled, Kivo opens the link summary choice directly. Kivo sends the selected link to Gemini only when you confirm the action. Webpages use Gemini's URL-context retrieval; YouTube links use its video input. These requests use the existing API key and model, with `store: false`, and do not require another service, a browser extension, or a local downloader. The result shows its source URL and can be copied; link summaries cannot replace the selection, including through the native command. Disabling Summarize in Settings keeps selected links in the ordinary action menu.

Link requests have a 90-second deadline; ordinary writing requests retain their 20-second deadline. Closing the popup cancels Kivo's pending request and discards late results. A request already received by Gemini may still consume quota. Public content may be unavailable to Gemini, and URL-context retrieval may use cached content. Private/unlisted YouTube videos and pages requiring sign-in or payment are unsupported. If retrieval fails or the request times out, close Writing Tools, select the article text or transcript, and invoke Summarize again. Kivo requires successful URL retrieval evidence before displaying a webpage summary.

URLs, supplied text, and summaries are kept in the current writing session, not saved as history or logged. Only the invoked source is sent; Kivo does not read browser tabs, cookies, or browsing history. Gemini's normal API usage limits and billing apply, and processing long videos can use more quota than summarizing pasted text.

For isolated browser verification while another worktree is running:

```sh
KIVO_UI_TEST_PORT=1437 bun run test:ui
```

The browser harness uses illustrative responses and never calls Gemini. Release verification must also exercise a real public article and public video with a configured key, along with native selection, focus, clipboard, and placement checks on macOS and Windows.

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

Without Apple signing credentials, the rolling `continuous` beta is ad-hoc
signed (`signingIdentity: "-"`, free). macOS TCC checks the code signature,
not just the bundle id, so rebuilt/updated betas can require renewed
Accessibility and Input Monitoring grants. System Settings may still list
an old build as enabled while the new copy is not trusted. Keychain may
also ask again after an update. Both beta and stable workflows use the
same Developer ID credentials when configured, providing a consistent
signing identity across updates.

A restart alone does not change the app signature and should not invalidate
approval. If the same build needs approval on every launch, quit Kivo,
eject its installer disk, and open the installed copy from Applications.
Check for duplicate copies in Downloads, Dock shortcuts and Login Items.
Gatekeeper's **Open Anyway** approval is separate from Accessibility,
Input Monitoring, microphone and Speech Recognition permissions.

What to check on the Mac:

- `codesign -dv --verbose=4 /Applications/Kivo.app` — `Signature=adhoc`
  means grants will not survive updates; a Developer ID line means they should.
- If an old build's entry is stuck and the new one can't be enabled, use
  Settings → Permissions → Clear stale entries (runs
  `tccutil reset All com.kivo.desktop` for Kivo only, no sudo needed),
  then re-allow each permission in turn. Manual equivalent:
  `tccutil reset All com.kivo.desktop`, then re-add Kivo in
  System Settings → Privacy & Security → Accessibility and Input Monitoring.
- If Kivo never appears in the Accessibility / Input Monitoring lists at
  all, no system prompt ever fired for that copy — and without a prompt
  there is no entry to toggle. Click Allow on the matching in-app row
  (Settings → Permissions, or onboarding) and watch for the system
  prompt; no prompt means no entry. Dev checkouts (`cargo run`,
  `tauri dev`) register under the debug binary path and every rebuild
  invalidates the entry, so verify permission behavior with the
  installed `/Applications/Kivo.app` copy and look for the full binary
  path in the list, not just "Kivo". On managed Macs an MDM profile can
  block these two services entirely: microphone and Speech Recognition
  working while Accessibility / Input Monitoring can never be enabled
  points there.
- `log show --last 10m --predicate 'process == "Kivo"'` and Console.app
  show the prompt / TCC denial lines; Kivo never logs text, transcripts,
  or keys.
- In-app, Settings → Permissions now refreshes automatically (poll +
  window focus) after you grant in System Settings; the first Allow click
  shows the system prompt, a still-off state afterwards means open System
  Settings and toggle Kivo there.

## Privacy and diagnostics

Do not add logs containing selected text, transcripts, Gemini responses, clipboard contents, or credentials. User-facing errors are deliberately short; local diagnostics should record only operation names, error categories, and non-sensitive OS codes.
