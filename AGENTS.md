# AGENTS

Project-specific guidance for AI coding agents.

<!-- ASTRYX:START -->
Astryx v0.6.2 · 164 components
CLI: run every command as `bunx astryx <cmd>` (shown below as `astryx ...`).

SETUP (once, in your app entry e.g. main.tsx) — without these, components render unstyled:
  import "@astryxdesign/core/reset.css";
  import "@astryxdesign/core/astryx.css";

WORKFLOW — discover, don't guess. Before writing UI:
1. `astryx build "<idea>"` — START HERE: returns a kit (closest [page] + [block]s + [component]s). No args = full playbook.
2. `astryx template <name> [--skeleton]` — scaffold the [page]/[block]s it named, or study their layout. Templates are reference code.
3. `astryx component <Name>` — props + examples for every component you use.

RULES:
- No <div> — components do all layout/spacing, page frame included.
- Frame first: read `astryx docs layout` before writing any page or screen — page frame, region widths, breakpoint behavior.
- Dense data = rows (Table, List/Item), never Card-wrapped list items; Card is for standalone widgets. Status = StatusDot/Token; Badge = counts only.
- Custom styling: component props first; else style/className with tokens — var(--color-*|--spacing-*|--radius-*). No raw hex/px. (No StyleX/Tailwind compiler here — don't use xstyle/utility classes.)
- Tokens for every value (`astryx docs tokens`). Brand/accent belongs in the theme (`astryx theme list` / `theme add <slug>`, or `astryx theme template` for a custom one) — never override --color-* in :root.
- SELF-CHECK before you finish: re-read the file and replace any raw <div>/<span> layout, imported .css/@apply, or hardcoded value (#hex, 16px) with the component or a token (var(--color-*|--spacing-*|…)). If unsure a component/prop exists, run `astryx component <Name>` / `astryx search "<thing>"`; don't hand-roll CSS.

MORE CLI:
  search "<query>"   find any component / hook / doc / template / block
  component --list   164 components by category
  template --list    page + block recipes
  docs <topic>       browser-support, cli-integrations, color, elevation, getting-started, icons, illustrations, internationalization, layout, migration, motion, principles, shape, spacing, styling-libraries, styling, theme, tokens, typography, working-with-ai
  swizzle <Name>     eject component source for deep customization
  upgrade --apply    run after any Astryx or integration dependency bump
<!-- ASTRYX:END -->

## Kivo overrides (these win over the generated block above)

The generated rules assume a no-build Astryx consumer. Kivo is not one: it runs
the StyleX compiler and defines its own Astryx theme.

- **StyleX is available and expected.** `@stylexjs/unplugin` runs in
  `vite.config.ts`. Author app layout with `import * as stylex from
  "@stylexjs/stylex"`, `stylex.create({...})`, and spread
  `{...stylex.props(styles.x)}` (or the `xstyle` prop on Astryx components).
- **`<div>`/`<span>` are allowed when they carry StyleX layout.** Use Astryx
  components for controls and surfaces (Button, Switch, Spinner, StatusDot,
  SegmentedControl, Card, TextInput, Banner, …); use StyleX-styled elements for
  layout, since Astryx `Stack`/`Card` don't cover every compact surface here.
- **Tokens, not literals.** Prefer `@astryxdesign/core/theme/tokens.stylex`
  vars or `var(--color-*|--radius-*|--spacing-*)` in StyleX values. Kivo-only
  tokens (`--kivo-*`: overlay palette, tertiary text, hover/selected) are
  declared in `src/theme/kivo.ts` under `localTokens`.
- **Theme changes go through the source, then the build.** Edit
  `src/theme/kivo.ts` and run `bun run theme:build`; commit the regenerated
  `src/theme/built/kivo.{css,js,d.ts,variants.d.ts}`. `bun run theme:check`
  fails CI when the committed artefacts are stale.
- **The compact overlays (flow bar, Writing Tools) are always dark.** They are
  wrapped in a nested `<Theme mode="dark">`; use the `--kivo-overlay-*` tokens
  instead of hard-coded dark colors.
- **Motion** (`motion/react`) owns entrance/exit and looping animations; keep
  `useReducedMotion()`/`MotionConfig` honoring the OS preference.
- **Generated markdown HTML** is the one exception to StyleX: `SafeMarkdown`
  outputs sanitized HTML, styled by `src/features/writing-tools/markdown.css`.

## Verification

Run the whole gate before finishing a change:

```sh
bun run check       # typecheck, Biome, Oxlint, Knip, theme:check, bun test, bridge/packaging/parity gates
bun test:ui         # Playwright browser harness
bun run build
bun check:rust && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked
```

Lefthook runs Biome + Oxlint on staged files at commit time and typecheck +
`bun test` on push. `bun install` installs the hooks.
