# Kivo — AI agent notes

This repo is maintained with AI help on a Linux server. Read this before changing code.

## Verify (Linux-safe, run these)

```sh
bun run ai-check        # typecheck + lint + fmt/clippy + vitest + 3 parity gates + cargo test (~40s)
bun run ai-check:full   # ai-check + vite build + playwright smoke (~2min)
```

Isolated browser run while another worktree serves the dev port:

```sh
KIVO_UI_TEST_PORT=1437 bun run test:ui
```

## Do NOT run here

- `bun tauri build` — needs macOS/Windows SDKs and signing. CI covers it.
- `cargo check --target <foreign>` — needs that desktop's C toolchain. CI's per-OS
  `native` matrix covers it instead.

## Parity rules (change both sides together)

- Shortcuts/defaults: `src/types.ts` <-> `src-tauri/src/config/mod.rs`
- AI models/blocklist: `src/ai/models.ts` <-> `src-tauri/src/ai/mod.rs`
- Bridge: `invoke("...")` in `src/platform/native.ts` must match a
  `#[tauri::command]` fn registered in `src-tauri/src/lib.rs`;
  `NativeEventMap` keys must be emitted by Rust;
  `AppSettings` fields must exist on `FrontendSettings`.
- The gates enforce this without compiling: `check:packaging`,
  `check:platform-parity`, `check:bridge`.

## Physical testing (never in CI)

Shortcuts (Fn/Win), mic/SAPI voices, AX/UIA paste, tray, signing — verify on
real macOS + Windows hardware before release.
