# Kivo repository health — 11 October 2026

Original audit scope: tracked repository at 86b55a8; frontend, Rust desktop, build/check scripts, dependencies and lockfiles, workflows, packaging, website, documentation, tests, and live GitHub run/branch-protection metadata. No application source, dependencies, workflows, or repository settings changed. This is a repository-level audit with targeted implementation review, not a claim that every native path or third-party dependency has been exhaustively reviewed.

## Remediation in PR #78

The findings below describe the original baseline, not the current branch. Implementation: https://github.com/0libote/Kivo/pull/78.

| Area | Implemented change | Evidence / remaining acceptance |
| --- | --- | --- |
| Continuous releases | Content- and commit-qualified immutable installers/signatures; manifest promoted last; main-only publication; freshness checked twice; old asset pairs retained | Interrupted/superseded upload tests. GitHub manifest replacement can briefly return 404; it no longer points at replaced installer bytes. Real signed-update acceptance remains in the release checklist. |
| CI cost | Shared verification for PRs and releases; fail-safe change classification; one full browser host; cheap frontend checks grouped; pinned Rust; profile caches saved only on main; release debug info disabled | Workflow/action lint; explicit success/skipped aggregate keeps the existing required check name. Cold/warm hosted timings must be compared after merge; no claimed speedup from unmeasured compiler caching. |
| Native coverage | Windows PR installer is installed, launched, checked for four hydrated/rendered WebViews, and uninstalled | Automated CI smoke; real microphone, insertion, shortcuts, GPU, DPI, and signed-update checks stay explicit in docs/releasing.md. |
| Installer execution | Ollama button opens the official download page; executable downloader/spawner and its bridge/events removed | Build/contracts/browser setup coverage. Local server probes run concurrently. |
| Model lifecycle | Rust per-model operation guards survive cancellation/verification/publication; Voz queues operations, invalidates obsolete loads, cancels timed-out/dropped requests, ignores late replies, and restricts worker IPC to the flow bar | Operation, serialization, native guard, request cleanup, and browser tests. SDK downloads cannot be forcibly aborted through the current upstream API; obsolete results cannot publish readiness. |
| Dependency bloat | Four root ML packages removed; full exact SDK declarations and license notices retained with tarball integrity and offline hashes | Two frozen clean installs: baseline 259 packages / 1,012 MB, branch 233 / 581 MB (about 43% less). These are installed disk sizes, not installer sizes. Weekly upstream provenance check. |
| Production bundle | Native/shared/mock adapters split; mock bridge and gallery compiled out of desktop builds; worker remains demand-loaded | Bundle gate rejects simulation/runtime leakage; production Windows frontend browser smoke covers four surfaces. |
| Contracts | Actual Rust serde snapshots typechecked by TypeScript; defaults compared on both hosts; priced fallbacks/provider metadata generated from Rust | Found and fixed inputPer1M/outputPer1M/monthlyLimitUsd IPC names. Cargo rejects stale snapshots; source-name checks retained only as supplemental guards. |
| Architecture | Settings sections/layout extracted; provider catalogue/pricing and tests extracted; AI/config tests moved; updater orchestration extracted | Existing behavior coverage retained. Further module splitting is maintenance work, not a prerequisite for correctness; avoid a broad mechanical rewrite. |
| Maintenance | Website local-link/anchor/HTML/JS gate; canonical full checks; dependency owner and patch removal criteria documented | Keep security/DOM/theme/audio dependencies that are used. Historical audit/design files retained and labeled. |

## Retained dependencies and follow-up policy

DOMPurify/marked/jsdom, StyleX/Astryx, Tauri, and active native speech/audio dependencies provide exercised functionality and stay. The Astryx patch has an order-independence regression test and is removed when upstream theme generation fixes that case. GLib's small soundness patch stays until Tauri's Linux dependency stack consumes a fixed release; its upstream iterator regression test stays with the vendored source. Dependency compiler warnings are upstream, not suppressed application failures.

Repository maintainer **0libote** owns weekly dependency-health failures and a monthly frontend update review while Dependabot issue [#16071](https://github.com/dependabot/dependabot-core/issues/16071) blocks Bun v2 lockfile updates. Review on the first Monday each month; next review 2 November 2026. Re-enable automated Bun updates only after a test PR proves lockfile compatibility. Recheck both local patches with each relevant dependency upgrade.

Remote speech runtime trust is explicit: exact top-level SDK/runtime versions, HTTPS jsDelivr delivery, recorded upstream tarball integrity for declarations, version-scoped completion markers, existing CSP, and native Windows acceptance. This does not provide a cryptographic lockfile for every CDN executable/model fetch. A fully offline/self-hosted SDK distribution should be a separate product/deployment decision with upstream licensing and runtime tests; vendoring declarations must not be presented as executable verification.

## Assessment

The largest problem is an expensive delivery pipeline around a growing product, not a broken frontend build. There are useful safeguards already: strict TS, formatter/linter separation, actual Windows compilation, Rust tests, dependency audits, bridge registration checks, model hashes, sanitized Markdown, Credential Manager integration, and a required aggregate check. Preserve those. Prioritize release correctness, native build/cache cost, missing desktop coverage, and optional ML complexity.

## Measured baseline

Latest main run: https://github.com/0libote/Kivo/actions/runs/38056713468 — approximately 16m38s end to end.

- Windows beta: 980s, including installer build 647s and cache save 209s.
- Linux native: 178s; apt dependencies 55s, Clippy 43s, tests 57s.
- Windows native: 147s; Clippy 30s and tests 30s.
- Windows frontend/browser: 125s; Ubuntu browser: 78s.
- Frontend: 17s; audit: 16s; packaging/contracts: 5s.

Recent PR run: https://github.com/0libote/Kivo/actions/runs/38055866961 — approximately 11m48s end to end. Windows native took 699s: Clippy 393s, tests 54s, debug installer 83s, cache save 54s. Linux native took 241s. Timings vary with cache and dependency changes; these are measured examples, not guaranteed future durations.

Installed node_modules is about 1,013 MB. The largest relevant directories are Desert Ant 211 MB, ONNX Runtime Web 139 MB, LiteRT 38 MB, Biome 124 MB, and Astryx 48 MB. These are local installed sizes, not installer or download-size measurements. Vendored GLib is 2 MB; historical design assets are only 304 KB.

The Linux production frontend builds in 2.58s after typechecking, with a 652.56 kB main JS chunk (197.10 kB gzip), 212.32 kB CSS (38.27 kB gzip), and lazy settings/onboarding/Markdown chunks. The Windows frontend branch also builds, emitting a 4.31 kB Voz worker. The heavy browser ML runtimes are fetched remotely, not bundled in these outputs.

## Highest-priority findings

### 1. Rolling release asset replacement is not atomic

Evidence: `.github/workflows/ci.yml`, `publish-continuous`, uses stable installer filenames with `gh release upload --clobber`, then overwrites continuous.json. Until the manifest changes, its old signature can point at the newly replaced installer; partial upload failures can leave that mismatch indefinitely. The comment promising that the old release stays available is stronger than the implementation. Publication serialization does not solve the reader-facing asset/manifest mismatch.

Change: give each build's installer and signature immutable SHA-qualified filenames, upload those, then switch the manifest. Retain a bounded number of old asset pairs and clean them only after successful publication. Test interrupted uploads and a client fetching during promotion. This is a correctness fix before a performance optimization.

### 2. Expensive beta builds run for superseded main commits

Evidence: windows-beta has no concurrency group; checks only cancel superseded PRs. Every main push starts a release build. The publisher discovers a stale SHA only after its full build and artifact upload. Manual dispatch from any branch is eligible and bypasses the main-head freshness check, so a branch build can replace the shared continuous channel.

Change: separate build from promotion; cancel superseded continuous builders by branch/channel, retain serialized promotion, restrict the shared channel to main, and make branch builds artifact-only. Validate freshness again immediately before promotion. Define whether manually dispatched main builds can replace a newer successful build.

### 3. The native build and cache path is the real CI bottleneck

Evidence: main compiles Windows debug/test artifacts in native and release artifacts separately in windows-beta. Clippy, tests, and bundling have different outputs, so they cannot simply share one final executable. The beta cache save alone costs 3m29s. Debug info is disabled for native checks but not for beta. Compiler toolchain is floating stable, despite the declared rust-version being 1.88.

Change: pin a Rust toolchain with rustfmt/clippy; explicitly separate check/test and release cache identities; inspect cache hit/miss and size before changing policy; trial disabling release debug information and unnecessary cache entries; save only on trusted main runs. Benchmark C/C++ compiler caching for transcribe-cpp and Rust caching separately. Consider joining Windows validation and beta packaging on main to reuse checkout, SDK, dependency installation, and available intermediates, but measure wall time: serializing all work can make delivery slower. Keep full-LTO tagged builds only if binary/startup measurements justify them.

Do not lead with replacing Vite, React, or Bun: the frontend gate is already seconds.

### 4. Browser tests do not validate the supported desktop

Evidence: Playwright starts Vite and uses MockBridge. Full suites run on both Ubuntu and Windows; user-agent branches already simulate both platforms. Live microphone/SAPI tests in windows_speech.rs are explicitly ignored. PR installer creation verifies bundling, not installation, launch, insertion, permissions, or updates. Three static gate scripts inspect source text, not behavior.

Change: run the full browser suite once, retain a small Windows-specific smoke suite if it catches real host issues, and add a Windows installation/launch smoke test. Maintain explicit real-desktop acceptance checks for text insertion, clipboard restoration, shortcuts, microphone/device changes, focus, multi-monitor placement, sleep/resume, and signed updates. Test the built frontend in addition to the dev server, including TAURI_ENV_PLATFORM=windows; Ubuntu production builds intentionally substitute a stub worker.

### 5. Ollama installation has avoidable execution and lifecycle risks

Evidence: src-tauri/src/ai/local.rs downloads a moving installer URL into a predictable temp filename, then launches it directly. There is no explicit publisher/signature verification, per-operation directory, timeout/size cap, cancellation, concurrent-install guard, or cleanup path. HTTPS is useful but does not constitute application-side verification of the executable being launched.

Change: simplest product decision is to open the official download page and let the user install it. If one-click installation remains, use a unique private staging directory, verify the expected Authenticode publisher before launch, set bounded download limits/timeouts, serialize requests, and clean up partial downloads. Add behavior tests around interruption and duplicate requests.

### 6. Model operations can race

Evidence: ModelStore::download replaces the cancellation flag for an existing model ID and writes the same .partial file without a per-model operation guard. ModelStore::delete does not coordinate with active downloads. The UI's busy state is not a backend synchronization guarantee. Voz's message handler starts async operations concurrently; remove can clear references/markers while an outstanding load later sets them ready again. Ear loading lacks the model-level in-flight promise that Voz loading has.

Change: enforce per-model single-flight downloads/deletes in Rust; coordinate cancellation through verification/rename; serialize destructive Voz operations and use a generation/cancellation token so an old load cannot resurrect removed state. Add concurrency tests for download/download, download/delete, install/remove, and timeout/late completion. These are code-visible race risks; they were not reproduced on Windows hardware in this audit.

## Dependency and product simplification

### 7. Optional ML tooling is the strongest dependency reduction opportunity

Voz/Ear are imported for types; LiteRT/ONNX are runtime URL strings rather than static imports. They are nonetheless installed at the root, bringing substantial WASM/native assets and transitive dependencies into every frontend lint/test job. The previous audit's blanket 'keep' decision does not establish that all these packages belong in every install.

Decide whether three speech engines (System, native on-device, browser Voz) are necessary for the next milestone. If Voz is experimental, isolate its adapter and development dependencies in a workspace or explicit integration fixture. Investigate removing direct ONNX/LiteRT root dependencies, including their peer-resolution effects, before claiming the saving. Move type-only SDK dependencies into an appropriate development/integration scope, and verify browser types against the exact runtime used. Do not handwrite loose types merely to make Knip green.

The remote runtime is outside bun.lock's integrity chain. Ear/Voz have pinned top-level URLs but +esm resolution and transitive runtime assets still deserve a recorded provenance/integrity strategy, offline behavior, versioned cache markers, CSP checks, and a Windows integration test. A dependency update does not automatically update hardcoded worker URLs.

### 8. Several packages are justified; removing them is low value

Keep jsdom for DOMPurify's real DOM semantics; moving its setup to DOM-specific tests may reduce test coupling. Keep DOMPurify and marked. Biome has its linter disabled and acts as formatter/import organizer; Oxlint is the linter, so this is not duplicate linting. Keep StyleX because app layout uses it. Keep theme-neutral: theme/kivo.ts imports it. Keep Tauri APIs and actively used Rust speech/audio dependencies unless the corresponding feature is removed. GLib carries a documented soundness patch: deleting it to reduce 2 MB is the wrong tradeoff.

Astryx CLI is useful for theme builds/discovery but carries more than the theme build needs. Investigate a lightweight deterministic theme-build entrypoint upstream. Maintain an expiry/removal criterion and focused regression coverage for both the Astryx patch and GLib patch.

### 9. Split production and simulation code deliberately

src/platform/native.ts contains both TauriBridge and a large MockBridge selected at runtime; App also imports the developer menu and emits a gallery chunk in production. Extract a shared bridge interface/events, native adapter, and explicit development/test adapter. Compile the latter and gallery/menu out of production when feasible. Measure startup and bundle composition before altering overlays: their early event subscriptions are intentional. A chunk warning alone does not justify arbitrary lazy loading. Preserve a ready/snapshot handshake if surface-specific entrypoints are introduced.

## Build, tooling, and maintenance

### 10. Consolidate verification without weakening it

frontend runs typecheck then build, whose first command runs typecheck again. Windows repeats typecheck/unit/bridge/contract checks and the full browser suite. packaging is a separate five-second job mostly repeating scripts. Native Linux PRs install Bun and dependencies but never execute an installer build: the workflow comment claiming PRs exercise installers on both hosts is inaccurate.

Create one canonical verification definition reused by CI and release workflows. Use build:frontend after an already successful typecheck. Fold cheap source gates into frontend. Keep Rust formatting/lockfile checks fast and Windows native compilation mandatory when applicable. Release currently omits format:check, Knip, check:bridge, Rust formatting, and browser tests; a tag or manual dispatch can therefore release a commit that never passed the complete required gate. Require verified provenance for the exact release SHA or run the shared release-required checks.

Rename the security-gate to reflect that it aggregates all checks, migrating branch protection at the same time. Live branch protection currently correctly requires it and enforces it for admins. Do not remove or rename that check without preserving protection.

### 11. Use change-aware jobs with explicit aggregate semantics

There are no path filters, so website/docs changes trigger the whole native/ML pipeline. Classify frontend, Rust, shared schema, packaging, workflow, and website changes. Include Cargo.lock, toolchains, build scripts, and configs in relevant filters. Make the always-running aggregate distinguish intentionally skipped jobs from failed classification or cancellation. GitHub-hosted runner minutes and time-to-first-failure are separate measures: parallel jobs improve latency while still spending minutes.

Use reusable setup for repeated Vulkan installation, Bun setup, cache parameters, and verification. Keep action SHA pinning. Add workflow syntax/action validation and shell linting. Capture cache hit, cache size, compile/link timing, installer size, bundle size, and test failures in job summaries so optimization is measured.

### 12. Shrink architectural hotspots by responsibility

SettingsWindow.tsx is 1,920 lines; commands/mod.rs 2,183; shell.rs 1,846; ai/providers.rs 2,853; ai/mod.rs 2,007. Counts include tests/data and are not proof of poor design, but the responsibilities make changes hard to isolate. Split settings sections/controllers, commands by domain, shell window/shortcut/update orchestration, and provider protocols/catalog/pricing. Preserve the existing behavior tests while moving code.

The bridge script checks names/registration/settings-field presence, not payload types, serde attributes, event shape, or runtime authorization. Contracts enforce exact source strings and function names, including CDN URLs and implementation snippets; harmless refactors can fail while a behavior bug passes. Prefer generated bindings/schema plus serialization roundtrip tests. Keep small invariants for packaging and registration until a replacement actually covers them.

AI model/provider catalogs and prices duplicated across Rust/frontend need a single source or explicit generated blocks and freshness checks. Existing model generation is a good starting point. Avoid expanding manual fallback tables independently.

### 13. Clean up maintenance scope, not tiny assets

The static website is outside Oxlint/Knip and the application build. It needs its own link/HTML/JS checks or a separate workspace/repository with clear ownership. Historical design notes are small; archive/label them rather than making their deletion a performance project. docs/repo-audit.md is a historical October 2 snapshot and its test counts/claims should not be treated as today's verification.

Use --locked in check:rust; add a pinned rust-toolchain.toml. Consolidate AGENTS/README/ai-check/full commands: ai-check:full uses only a browser smoke subset, while the documented full gate asks for all browser tests and repeats Rust checking before Clippy/tests. Document quick checks versus authoritative gates explicitly. Keep dependency-health failures visible; frontend automated updates are currently disabled, so establish an owner and review date rather than relying solely on vulnerability audits.

## Proposed implementation order

1. Correct immutable beta assets/channel promotion, installer execution, and concurrent model operations. Add targeted regression tests.
2. Record cache metrics, pin Rust, simplify cache saves, deduplicate frontend checks, introduce change classification and a shared release gate.
3. Isolate optional browser speech integration and remove proven unnecessary root ML installs; measure frozen-install size/time before and after.
4. Add Windows installation/launch/update acceptance coverage and production frontend smoke coverage; reduce redundant mock browser execution.
5. Refactor bridge/settings/shell/provider hotspots, generate contracts, and give website checks an owner.

Success measures: no mixed beta manifest/assets; no late model resurrection or shared partial-file writes; a green exact-SHA release gate; docs-only changes avoid native builds; visible cache hit/size metrics; smaller root dependency footprint; passing real Windows smoke checks. Establish targets after two or three representative baseline runs rather than promising a fixed percentage improvement.

## Original audit verification and limitations

Local /usr/bin/bun fails because its npm wrapper's postinstall was not run. /home/oliver/.bun/bin/bun is a working 1.4.2 installation and was used via PATH for verification; repository files were not changed to work around this environment issue.

bun run check passes: 60 unit tests and all TS/lint/Knip/theme/bridge/packaging/contracts gates. Linux and Windows-mode frontend production builds pass, with the main-chunk warning. Rust formatting, locked all-targets cargo check, and Clippy pass; all 134 Rust tests pass on Linux (the patched GLib emits 34 dependency warnings). The initial browser run had 26 passes and 3 failures across 29 tests, including dynamic-import fetch errors. A fresh-port, single-worker rerun of the affected two test files passed all 9 tests in 28 seconds. This points to environment/dev-server/parallelism sensitivity, but does not establish the exact cause or a clean full-suite result. Add an explicit harness-ready handshake and reliable server isolation rather than relying on CI retries. Initial failures must not be silently counted as passing. No supported Windows hardware, signed installation/update test, installer-size measurement, or complete security audit was performed. GitHub timings and protection were read live; no PR, workflow run, release, or settings mutation was created.
