# Kivo: project review and Windows redesign plan

> Implementation update (12 September 2026): the overhaul is now in the working tree. See [implementation status and validation](IMPLEMENTATION.md) for what changed, verified screenshots, and the remaining native release checks. The findings and source line numbers below describe the pre-overhaul code.


Reviewed 12 September 2026. Direction agreed: **native Windows feel first; retain Tauri if it delivers.**

## TL;DR

Kivo has a worthwhile foundation. Keep the small React frontend, Rust core, native integrations, and direct optional Gemini connection. The immediate risk is correctness at the boundary with other applications: selection capture, replacement, cancellation, and clipboard preservation. Fix these before a broad launch.

The new visual direction is **quiet confidence**: warm neutral surfaces, graphite controls, comfortable type, a useful home screen, and a temporary dictation indicator. Windows should own the window behaviour. The app should disappear into the workflow when it is not needed.

1. **First:** make text delivery safe, cancellable, and recoverable.
2. **Next:** make Windows chrome, focus, tray, installation, and updates consistent.
3. **Then:** apply the new interface to the four existing surfaces.
4. **Release:** only after installed Windows tests and measured performance.

[Open the interactive design study](design-direction.html). It is an isolated proposal with sample states; it does not record audio, call Gemini, or modify app preferences. Its native caption controls are illustrative, not a proposed HTML implementation of Windows window controls.

## Scope and evidence

Reviewed the app shell, Windows platform implementation, speech/text adapters, command lifecycle, settings, frontend surfaces, packaging, and CI. Inspected browser renders of the current Settings and Writing Tools surfaces. Findings below are traced code defects or explicitly identified validation gaps; this was not a physical end-to-end dictation test in third-party applications, a complete security audit, or a full macOS platform audit. The separate marketing website was not reviewed in depth.

Baseline checks on this Windows workspace:

| Check | Result |
|---|---|
| TypeScript type checking | Passed |
| ESLint | Passed |
| Frontend unit tests | 21 passed |
| Rust tests | 39 passed |
| Playwright UI tests | 13 passed |
| Production frontend build | Passed |
| Packaging parity script | Passed, but its contract contradicts the README |

The build produced approximately 293 KB of JavaScript, 88 KB gzip. This measures the frontend bundle, **not** installed size, working memory, startup latency, or the WebView2 process tree.

The new concept has not been visually verified: the in-app browser was unavailable, and automatic approval review rejected the later local browser rendering/check command with “blocked by policy.” Existing UI tests and the current-interface captures completed before that rejection.

## What needs fixing

### P1 — Capture the selection before activating Writing Tools

**Evidence:** `src-tauri/src/shell.rs:499` calls `show_surface(..., true)` before `core.open_writing_tools()`. `show_surface` calls `set_focus`. Selection capture then reads the currently focused UI Automation element or sends Ctrl+C to the foreground app (`platform/adapters.rs:72`, `platform/windows.rs:111`).

**Failure:** invoking Writing Tools over a selection can move focus into Kivo before capture, resulting in an empty/wrong context. This also contradicts the capture function's explicit precondition.

**Fix:** capture the original target and selection first, then activate the popup. If immediate feedback is essential, show a genuinely non-activating pending surface; calling `show()` on an ordinary activating window is not a sufficient guarantee.

**Acceptance:** select text in Notepad, Word, Edge, and VS Code; invoke by shortcut; confirm exact source text and target identity. Test no selection separately. Add a shell-order regression check; browser mocks cannot validate the native ordering.

### P1 — Do not turn a stale-selection refusal into an unconditional paste

**Evidence:** `platform/windows.rs:214` rejects replacement when the selection or process has changed. `platform/adapters.rs:131` catches that `InvalidState` alongside unsupported/OS errors and invokes `paste_replacement`. The Windows paste path focuses the stored HWND and sends Ctrl+V without revalidating the selection.

**Failure:** a deliberate safety refusal is bypassed; a changed selection or caret can receive the replacement. Broad fallback after a partially successful input operation also warrants testing for duplicate insertion.

**Fix:** propagate stale-selection errors. Fallback is only appropriate for a supported failure category where the original target is still provably valid. Otherwise keep the result available and offer explicit Copy. Never automatically paste just because validation failed.

**Acceptance:** change selection while an AI result is pending; replacement must leave the document unchanged and retain the result. Test focus failure and partial input failure separately.

### P1 — Keep cancellation valid through processing, and validate the insertion target

**Evidence:** `shell.rs:535` unregisters Escape before speech finalization/AI cleanup. `commands/mod.rs:172–211` awaits speech and optional cleanup, then inserts into the current cursor without a cancellation-generation check or original target validation. Cancellation changes the state machine but does not abort this pending insertion path. The Windows insertion implementation sends Unicode keyboard input to whichever control currently has focus.

**Failure:** Escape stops being a useful cancel during processing. If cancellation arrives through another path, the pending operation can still insert text before `complete()` discovers an invalid state. Switching apps during the up-to-four-second cleanup wait can send the text to a different app. A delayed success-hide task can also hide a newer session because it lacks a session check.

**Fix:** reuse the writing request generation/cancellation approach for dictation. Retain Escape until completion, invalidate late results and hide timers, and verify the intended window/control immediately before delivery. Preserve the last transcript in memory for explicit Copy or retry when delivery is uncertain; clear it on exit and provide Clear. This is session recovery, not persistent transcript history.

**Acceptance:** cancellation during recognizer startup, finalization, and AI delay causes zero subsequent insertions; changing apps preserves recoverable text; rapid consecutive sessions do not hide or overwrite one another.

### P1 — Preserve the clipboard, including non-text formats

**Evidence:** `platform/windows.rs:111` and `:154` back up only Unicode text; `:585` empties the clipboard before writing; the restore helper does nothing when there was no text. The comment claiming non-text content is never destroyed is incorrect. Restoring plain text also discards accompanying rich text/HTML formats. The sequence-number check is useful but does not fix format loss or make the full operation atomic.

**Failure:** a copied image, file, or formatted passage can be lost after Kivo's fallback. A clipboard read failure is also treated like an absent text backup.

**Fix:** prefer direct text APIs. For fallback, use a deliberately supported clipboard preservation strategy with ownership/change checks. Refuse automatic fallback when the original clipboard cannot be safely preserved; offer Copy instead. Separate a busy/unreadable clipboard from an empty one. Fix allocation cleanup on failed clipboard writes as part of this work.

**Acceptance:** image, file list, HTML+text, empty clipboard, busy clipboard, and another application's concurrent copy retain their expected contents.

### P1 — Test the installation path that actually supports speech

**Evidence:** README requires Windows 11 24H2 and package identity for speech. `AppxManifest.xml:19` and the packaging check instead pin Windows 10 build 17763. The rolling beta CI publishes an NSIS installer (`.github/workflows/ci.yml:351`) without the MSIX identity used by the production path.

**Impact:** a passing packaging check does not prove that the advertised Windows dictation flow works in the distributed beta. Microsoft documents an MSIX package identity requirement for this speech path. This is a release validation gap; the installed beta was not exercised in this review.

**Fix:** choose one supported Windows floor and enforce it across documentation, installer, runtime checks, and CI. Make packaged speech testing part of the beta route too, or clearly mark an unpackaged build as UI-only. Ship a signed, stable identity; test upgrade and uninstall with that identity. Do not lower the support floor solely because WebView2 can run there.

### P2 — Make readiness and recording feedback truthful

**Evidence:** `platform/windows.rs:235` returns Granted for microphone and speech without checking. Requests for those permissions are no-ops. Windows speech does not emit `AudioLevel` events, although the Flow Bar depicts a level-based waveform. The `require_on_device: true` option supplied by the adapter is not enforced by the Windows implementation.

**Fix:** add a short real dictation exercise to onboarding with actionable speech/microphone errors and links to Windows settings. Use an honest activity indicator unless real levels are available. Do not promise offline processing based on the option's name; verify recognizer behaviour and document the Windows speech privacy boundary accurately. The operating-system engine and Gemini are separate processing paths.

**Acceptance:** disabled microphone, missing language, denied speech consent, and network loss each show a specific recoverable state. No generic Ready claim when the prerequisites are unknown.

### P2 — Apply settings consistently across runtime, disk, and all windows

**Evidence:** `commands/mod.rs:952` changes autostart/shortcuts before persistence. The configured `DeferredSettingsRuntime` does nothing, so the nominal rollback in `save_settings` does not roll back the real effects. `useSystemPreferences.ts` reads settings once and does not subscribe to the emitted `settings-changed` event. All four webviews are pre-created. `shell::apply_settings` also changes Flow Bar visibility regardless of the current dictation phase.

**Failure:** a save error can leave the runtime different from stored preferences. Already-created windows can retain old themes and enabled actions. Saving a preference while dictating can hide or reset the indicator.

**Fix:** apply and roll back actual runtime effects in one transaction boundary; subscribe each surface to settings updates using the existing event hook; update idle visibility only when idle. Handle initial load errors visibly. Route the tray's `show-about` event too; currently Settings has no listener for it.

**Acceptance:** simulate disk failure and shortcut conflict; runtime and UI retain the previous values. Theme/action changes reach every surface. Saving unrelated preferences does not interrupt dictation feedback.

### P2 — Complete the Windows shell integration

**Evidence:** `platform/windows.rs:269` applies `DWMSBT_TRANSIENTWINDOW` to every styled window, including Settings. Onboarding is not included in the shell's styling loop. No explicit DWM border-colour or caption-theme policy exists. Styling errors are discarded. Main windows already have native decorations; Flow Bar already has `WS_EX_NOACTIVATE`, and both overlays already skip the taskbar.

**Fix:** preserve those good foundations. Use a main-window material or plain opaque surface for persistent windows; reserve transient treatment for popups. Set neutral/suppressed DWM borders explicitly, update dark/light caption treatment, and respect high contrast. A DWM border setting removes window chrome colour; it must not remove keyboard focus indicators. Avoid a translucent rectangular HWND behind a smaller CSS pill. Validate the actual composed desktop result before choosing materials.

**Acceptance:** Settings and onboarding support expected caption, resize, Snap, Alt+Tab, and DPI behaviour. Only active popups sit above other windows. Recording never steals focus; hidden windows intercept no clicks. Closing Settings hides it; Quit exits. Tray pause state and menu labels reflect reality.

## What is good, and what is merely unfinished

| Foundation | Keep | Improve |
|---|---|---|
| Rust/React boundary | Rust owns credentials, OS access, AI, and lifecycle | Fix concurrency/target contracts before cosmetic refactoring |
| Dependencies | Just React, React DOM, and Tauri API at runtime on the frontend | No UI kit or second state framework needed for the redesign |
| Credential handling | Credential Manager/Keychain, redacted secret debug output | Continue verifying no secrets or source text reach diagnostics |
| AI requests | Explicit actions, stateless requests, deadlines, raw dictation fallback | Add honest integration checks against the configured model; current fixtures do not prove provider availability |
| Writing request lifecycle | Generation checks, cancellation, source-aware summary restrictions | Carry the same discipline into dictation and target delivery |
| UI | Consistent icons, short onboarding, semantic controls, reduced-motion handling | More readable type, better action hierarchy, high-contrast and native accessibility verification |
| Shell | Tray, single instance, background start, native main-window decorations | Correct activation order, robust tray state, window-theme consistency |
| CI | Frontend, native, UI, security and packaging checks already exist | Add real installed Windows smoke coverage; include the Windows frontend job in release eligibility; label-triggered PR builds need a `labeled` event |

Avoid a speculative backend rewrite or a whole-repo abstraction cleanup. Extract smaller modules only when the reliability changes make the ownership boundary clearer. Four eager webviews may cost idle memory, but benchmark first; lazily creating every window can worsen the core hotkey experience.

## Design: quiet confidence

The current interface is already fairly restrained. Its weakness is hierarchy and finish: small text, configuration as the landing experience, many equally prominent writing actions, and inconsistent material treatment. The overhaul should make the purpose immediately obvious.

Take Wispr Flow's separation between a main hub and an in-context dictation surface as inspiration. Kivo does not need its accounts, word-count gamification, referrals, or broad feature menu. Keep Kivo's own compact identity and privacy model.

| Surface | Proposed experience |
|---|---|
| Main window | Home, Dictation, Writing tools; Preferences at the bottom. Home shows readiness, the actual shortcut, and a practice entry point. Show recovery only when a transcript needs it. |
| Preferences | Plain grouped rows, useful labels, minimal box nesting. Gemini setup lives here alongside privacy/help/about. System theme by default. |
| Writing popup | Custom instruction at the top; Proofread, Rewrite, Shorten as primary actions; tone and summaries under More. Preserve source context. Explicit result recovery when replacement is unsafe. |
| Dictation | Compact solid graphite indicator above the taskbar, visible only during invocation, listening, finishing, success, or actionable error. Idle remains hidden by default, as it already is. |
| Onboarding | Welcome → prove dictation with a real sentence → optional Gemini. Windows gets Windows-specific readiness checks. Mac keeps its platform permission flow. |

Visual specifications:

- Segoe UI Variable on Windows, using the installed platform font. Native familiarity takes precedence over choosing an unusual brand font.
- Body/control text around 14px; secondary text 12–13px; section headings 24–28px; home headline 30–32px. Support Windows text scaling rather than freezing the screen to these sizes.
- Warm white `#FCFCF9`, soft rail `#F1F1EC`, graphite `#242622`, muted green `#4A664C` for meaningful status. Dark equivalents are in the concept.
- Restrained 4/8px spacing rhythm, roughly 36px content inset, modest 6–9px corners, quiet dividers. Use one layer of elevation per popup.
- Neutral primary controls and selection fills. No permanently coloured window frame. Retain visible keyboard focus and system high-contrast colours.
- Short opacity/position feedback only when it communicates state. No perpetual decorative animation or simulated audio levels.
- Material and window behaviour belong to native code. CSS provides the content, not simulated window-management controls.

The concept demonstrates composition, navigation, colour, and prioritization. It is not a claim that Home, practice mode, recovery, or the proposed menu organization already exists. All current actions must remain reachable during migration. Error and success states need a separate focused design pass before implementation is considered complete.

## Windows architecture decision

**Keep Tauri.** This matches the user's stated priority. Tauri uses system WebView2 on Windows, so Kivo can have native window management and OS integrations while its content remains web-rendered. It is not a fully native WinUI control tree, and styling cannot make that claim true.

Use the existing Windows/Rust adapter for DWM attributes, target handling, tray and shortcuts. Reuse the current frontend components with a revised token/spacing system. Do not introduce a service, privileged helper, or extra shell framework without a measured requirement.

A WinUI 3 shell becomes a separate, evidence-based option only if native accessibility, rendering, startup, or memory targets cannot be met. If needed, preserve the Rust domain logic and replace the Windows presentation host through a small explicit boundary. That would be a substantial platform-specific migration, not a dependency swap; it is not part of the agreed first pass.

## Delivery sequence and definition of done

### 1. Reliability pass

- [ ] Fix capture-before-focus and stale-selection fallback.
- [ ] Make dictation startup/processing/cancellation one coherent session lifecycle.
- [ ] Validate delivery target and retain one in-memory transcript for recovery.
- [ ] Preserve clipboard formats or fail safely without automatic paste.
- [ ] Make settings effects transactional and synchronize all webviews.

Done when focused regression tests cover the actual failures and the native text-delivery matrix passes. Use deterministic delayed speech/AI fakes to test cancellation; avoid tests that merely repeat the state machine's implementation.

### 2. Windows shell pass

- [ ] Establish the supported OS floor and installable speech-capable beta path.
- [ ] Apply neutral DWM borders and correct per-window materials/theme handling, including onboarding.
- [ ] Verify tray pause/resume, background launch, Quit, focus, Snap, taskbar, and Alt+Tab.
- [ ] Reconcile packaged startup tasks with the actual autostart implementation.
- [ ] Exercise signed install, upgrade, uninstall, and the Windows update handoff.

Done when the installed app behaves consistently with Windows conventions; a browser preview cannot close this milestone.

### 3. Interface pass

- [ ] Visually verify and refine this concept in light/dark/high contrast.
- [ ] Apply shared tokens and preferences layout first, retaining existing component behaviour.
- [ ] Add the useful Home/practice path with real readiness rather than a static green label.
- [ ] Simplify Writing Tools hierarchy while preserving keyboard access and all existing actions.
- [ ] Finish dictation/error/recovery and onboarding states.
- [ ] Verify text scaling, tab order, Narrator, keyboard focus, and reduced motion.

Done when every surface and its loading/error/empty/disabled state works at the actual native window sizes. Retain native title bars and resizing.

### 4. Release evidence

Measure an installed release build on a documented baseline Windows 11 machine. Include all Kivo-owned WebView2 processes in memory figures. Proposed initial targets, to confirm after baseline measurement: warm shortcut feedback p95 under 100ms, main-window reopening under 250ms, and idle CPU under 0.5% over five minutes. Record idle working set and startup baseline first; set a realistic memory budget from those measurements. Measure speech finalization, optional AI cleanup, and insertion separately so recognizer/network latency is not mistaken for UI latency.

Manual matrix: Notepad, Word, Edge, VS Code, and an elevated target that should fail safely; 100/150/200% DPI; mixed-DPI monitors; taskbar positions; light/dark/high contrast; keyboard/Narrator; sleep/resume; microphone unplug/reconnect; offline operation; rapid hotkeys; denied consent; changed selection; clipboard contention; cancelled and timed-out requests.

Persistent history, dictionary, snippets, accounts, cloud sync, additional AI providers, and a native UI rewrite are deferred. None is needed to prove the core interaction or this visual direction.

## Sources

- [Wispr Flow's desktop navigation](https://docs.wisprflow.ai/articles/5096240724-navigating-the-wispr-flow-app-desktop-ios-and-android): inspiration for a useful hub plus a temporary in-context surface.
- [Tauri process model](https://v2.tauri.app/concept/process-model/): system WebView2 architecture on Windows.
- [Microsoft DWM window attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute): border colour, caption/dark-mode and system-backdrop controls.
- [Microsoft speech recognition constraints](https://learn.microsoft.com/en-ie/windows/apps/develop/input/define-custom-recognition-constraints): package identity requirement for speech recognition.
- [Microsoft Windows app development](https://learn.microsoft.com/en-us/windows/apps/): WinUI 3 / Windows App SDK as the native-shell alternative.
