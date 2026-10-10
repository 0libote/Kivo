# Repository audit

Audit date: 2026-10-02. This records evidence and cleanup decisions, rather than treating unfamiliar files as unused.

## Active code and dependencies

The initial `bun run check` passed, including Knip, typechecking, linting, theme freshness, 53 unit tests, and bridge/packaging/platform parity checks. The initial browser suite passed all 27 tests. Knip found no unused files, exports, or dependencies in its configured TypeScript project.

Knip is not a complete repository inventory: the static website is outside its project, and Rust/native code is outside its analysis. This audit removed the blanket `scripts/*.ts` entry pattern: package scripts and recognized workflow references now establish usage, with only the PowerShell-invoked updater-signature script explicitly listed. Unreferenced script helpers can now be detected. The stricter configuration also passes. The bridge gate checks frontend commands against native definitions and registration, but does not prove every native function is exercised on a real desktop.

| Area | Decision | Evidence |
| --- | --- | --- |
| `src/` | Keep | App entry, surface imports, unit/browser tests, and clean Knip result. |
| `scripts/` | Keep | Package scripts and CI reference build/check/release scripts; `ai-model-codegen.ts` supports the model generator. |
| `src/theme/built/` | Keep generated artifacts | Imported at runtime and checked against the source by `theme:check`. |
| Voz/Ear/ONNX/LiteRT dependencies | Keep | Worker types and Windows speech integration; absence from the normal frontend bundle is intentional. |
| `src-tauri/vendor/glib-0.18.5/` | Keep | Cargo explicitly patches GLib to carry a soundness fix; rationale is in `PATCH.md`. |
| `website/` | Keep as a separate static site | Independent pages, styles, and deployment instructions; it is not part of Vite's app build. |
| `docs/review/` | Keep as historical design reference | Screenshots and prior review notes; theme source still references the design notes. These are not current release instructions. |
| `eslint.config.js` | Removed | No ESLint command or installed `typescript-eslint`; Biome and Oxlint are the active tools. |
| `.tools/`, `node_modules/`, `dist/`, `src-tauri/target/`, `test-results/` | Keep ignored | Local tools, dependencies, and generated build/test output, not shipped source. Most local disk use comes from dependencies and Cargo output. |

## Fixes from the audit

- Dictation now applies native snapshots directly, so a missing earlier event cannot leave the overlay stuck in a previous phase. Non-finite microphone levels are normalized before animation.
- The flow bar uses an Astryx finish control with a duplicate-click guard and theme tokens for its styles. Its native state updates remain immediate instead of being deferred by an async React transition.
- Writing Tools ignores animation-only style mutations and deduplicates native height reports. Content resizes and text changes remain observed.
- The writing menu moves actual keyboard focus with its selection and uses one tab stop. Copy feedback replaces its previous timer and clears it on unmount.
- The root README is a short entry point. Detailed user, development, and release material lives in separate guides. Summary instructions and Windows Voz runtime descriptions now match the implementation.
- Beta release notes are readable Markdown maintained in `packaging/beta-release-notes.md`. CI appends version/build metadata. Stable draft notes identify installers and updater files.
- Removed the duplicate Cargo-output ignore entry and corrected Rust verification commands to use the actual manifest location.
- Tightened Knip's script entry points so future unused helpers are not automatically exempt from the audit.

## Verification after the changes

`bun run check` passes with the stricter Knip configuration and 55 unit tests. All 28 Playwright tests pass, including the new keyboard-focus and native-resize regression. `bun run build`, Rust formatting, `cargo check --locked --all-targets`, and Clippy with `-D warnings` pass. All 132 Rust tests pass. Bun's dependency audit reports no known vulnerabilities across 439 packages. The static website's local links/anchors, both release workflow YAML files, and the beta publishing shell syntax were also checked.

The frontend build retains its existing chunk-size warning. The vendored GLib dependency emits compiler warnings; Kivo's Clippy gate passes. Linux verification does not replace the supported-desktop checks below. Release wording changes apply when the workflows next publish; no remote release was edited during this audit.

## Release checks still requiring real desktops

Browser tests simulate the bridge. They cannot certify native permission prompts, global shortcut suppression, capture/insertion in third-party editors, clipboard restoration, microphone selection, multi-monitor placement, or signed installation and updates. Exercise these on supported Windows hardware before publishing a stable release.

The production frontend build also reports a chunk-size warning. Profile startup and inspect bundle composition before splitting eager overlays: their event listeners intentionally mount early, so indiscriminate lazy loading can lose native events.

Future cleanup should use imports, workflow references, native registrations, and executable checks as evidence. Do not remove generated themes, platform-specific workers, native bridges, or the vendored security fix merely because the Linux/browser harness does not use them.
