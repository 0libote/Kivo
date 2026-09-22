import { defineTheme } from "@astryxdesign/core/theme";
import { neutralTheme } from "@astryxdesign/theme-neutral";

/**
 * Kivo's Astryx theme. It extends the neutral base (warm restrained grays,
 * Lucide icon registry) and re-seats the palette on Kivo's graphite tokens:
 * a near-black accent in light mode and near-white in dark, so primary
 * controls read as native rather than brand-colored.
 *
 * Values here mirror the design notes in docs/review/POLISH.md. Build with
 * `bun run theme:build`; the generated kivo.css/kivo.js are committed and
 * imported by src/main.tsx.
 */
export const kivoTheme = defineTheme({
  name: "kivo",
  extends: neutralTheme,
  // No `color.accent` seed: the HCT palette generator assigns a hue to a
  // chroma-free seed, which would turn Kivo's graphite accent blue. The
  // accent and its reference tokens are overridden directly instead.
  color: {
    neutralStyle: "warm",
  },
  typography: {
    scale: { base: 14, ratio: 1.2 },
    body: {
      family: "system-ui",
      fallbacks: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', sans-serif",
    },
    heading: {
      family: "system-ui",
      fallbacks: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', sans-serif",
    },
  },
  radius: { base: 4, multiplier: 1 },
  motion: { fast: 120, medium: 170, ratio: 0.8 },
  tokens: {
    "--color-background-body": ["#f7f7f8", "#19191b"],
    "--color-background-surface": ["#f4f4f5", "#202023"],
    "--color-background-card": ["#ffffff", "#252528"],
    "--color-background-popover": ["#ffffff", "#252528"],
    "--color-background-muted": ["#f0f0f1", "#2a2a2d"],
    "--color-text-primary": ["#1d1d1f", "#f3f4f5"],
    "--color-text-secondary": ["#626267", "#b0b4bb"],
    "--color-border": ["rgba(29, 29, 31, 0.11)", "rgba(255, 255, 255, 0.12)"],
    "--color-border-emphasized": ["rgba(29, 29, 31, 0.18)", "rgba(255, 255, 255, 0.2)"],
    "--color-accent": ["#252527", "#f2f2f3"],
    "--color-on-accent": ["#ffffff", "#1d1d1f"],
    "--color-text-accent": ["#252527", "#f2f2f3"],
    "--color-icon-accent": ["#252527", "#f2f2f3"],
    "--color-accent-muted": ["rgba(37, 37, 39, 0.08)", "rgba(242, 242, 243, 0.12)"],
    "--color-success": ["#34845a", "#70d49a"],
    "--color-error": ["#c7372f", "#ff716b"],
    "--color-warning": ["#8a6100", "#e8c06a"],
    "--color-overlay": ["rgba(29, 29, 31, 0.5)", "rgba(0, 0, 0, 0.6)"],
  },
  localTokens: {
    // Kivo-only values: a third text weight and the shared row/surface states
    // the surfaces used to keep in globals.css.
    "--kivo-text-tertiary": ["#74747a", "#989ea7"],
    "--kivo-text-quaternary": ["#838a94", "#747b85"],
    "--kivo-hover": ["rgba(29, 29, 31, 0.055)", "rgba(255, 255, 255, 0.085)"],
    "--kivo-selected": ["#e8e8ea", "#323235"],
    "--kivo-signal": ["#34845a", "#90e7b7"],
    // The compact overlays (flow bar, Writing Tools) are always dark,
    // independent of the window theme.
    "--kivo-overlay-bg": "#16171afa",
    "--kivo-overlay-border": "rgba(255, 255, 255, 0.1)",
    "--kivo-overlay-border-strong": "rgba(255, 255, 255, 0.16)",
    "--kivo-overlay-hover": "rgba(255, 255, 255, 0.08)",
    "--kivo-overlay-selected": "rgba(255, 255, 255, 0.12)",
    "--kivo-overlay-text": "#f5f5f6",
    "--kivo-overlay-text-secondary": "#a8abb1",
    "--kivo-overlay-text-tertiary": "#92959c",
    "--kivo-overlay-shadow":
      "0 18px 48px rgba(0, 0, 0, 0.3), inset 0 1px rgba(255, 255, 255, 0.06)",
  },
});
