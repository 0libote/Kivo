/**
 * Single source of truth for the Gemini model catalog: `src-tauri/src/ai/mod.rs`.
 *
 * This module parses `SUPPORTED_GEMINI_MODELS` / `BLOCKED_MODEL_SUBSTRINGS`
 * out of the Rust source and renders the TypeScript mirror used by
 * `src/ai/models.ts` (offline fallback + harness list). Run
 * `bun run generate:models` after editing the Rust table; CI fails the
 * platform-parity gate when the generated section is stale.
 *
 * Zen/Go/Custom priced fallbacks stay hand-curated per side: their TS rows
 * carry precomputed cost strings while Rust composes them from price tables
 * via `cost_label_for`, so a literal mirror is impossible. The parity script
 * still asserts their invariants (every row priced, provider defaults
 * present).
 */

export const GENERATED_START = "// @generated ai-models start";
export const GENERATED_END = "// @generated ai-models end";

export interface GeminiModelRow {
  id: string;
  label: string;
  description: string;
}

function unescapeRust(value: string): string {
  return value.replace(/\\(.)/g, "$1");
}

function sliceConst(source: string, name: string): string {
  const anchor = source.indexOf(`pub const ${name}`);
  if (anchor === -1) throw new Error(`const ${name} not found in Rust source`);
  const end = source.indexOf("];", anchor);
  if (end === -1) throw new Error(`const ${name} has no closing ];`);
  return source.slice(anchor, end);
}

/** Parse the `SUPPORTED_GEMINI_MODELS` table in order. */
export function parseGeminiModels(rustSource: string): GeminiModelRow[] {
  const block = sliceConst(rustSource, "SUPPORTED_GEMINI_MODELS");
  const rows: GeminiModelRow[] = [];
  const pattern =
    /id:\s*"((?:[^"\\]|\\.)*)"\s*,\s*label:\s*"((?:[^"\\]|\\.)*)"\s*,\s*description:\s*"((?:[^"\\]|\\.)*)"/g;
  for (const match of block.matchAll(pattern)) {
    rows.push({
      id: unescapeRust(match[1]),
      label: unescapeRust(match[2]),
      description: unescapeRust(match[3]),
    });
  }
  if (rows.length === 0) throw new Error("no Gemini model rows parsed");
  return rows;
}

/** Parse the `BLOCKED_MODEL_SUBSTRINGS` list in order. */
export function parseBlockedPatterns(rustSource: string): string[] {
  const block = sliceConst(rustSource, "BLOCKED_MODEL_SUBSTRINGS");
  const patterns = [...block.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  if (patterns.length === 0) throw new Error("no blocked patterns parsed");
  return patterns;
}

const tsString = (value: string): string => JSON.stringify(value);

export function renderFallbackRows(models: GeminiModelRow[]): string {
  const rows = models
    .map((m) => `  [${tsString(m.id)}, ${tsString(m.label)}, ${tsString(m.description)}],`)
    .join("\n");
  return `const FALLBACK_ROWS: Array<[id: string, label: string, description: string]> = [\n${rows}\n];`;
}

export function renderBlockedPatterns(patterns: string[]): string {
  const rows = patterns.map((p) => `  ${tsString(p)},`).join("\n");
  return `export const BLOCKED_AI_MODEL_PATTERNS = [\n${rows}\n] as const;`;
}

export interface GeneratedSections {
  fallbackRows: string;
  blockedPatterns: string;
}

export function renderGeneratedAiModels(rustSource: string): GeneratedSections {
  return {
    fallbackRows: renderFallbackRows(parseGeminiModels(rustSource)),
    blockedPatterns: renderBlockedPatterns(parseBlockedPatterns(rustSource)),
  };
}

/** Extract the current generated region (between the marker pair at index). */
export function extractGenerated(tsSource: string, occurrence: number): string {
  let from = -1;
  for (let i = 0; i <= occurrence; i++) {
    from = tsSource.indexOf(GENERATED_START, from + 1);
    if (from === -1) throw new Error(`generated start marker #${occurrence} missing`);
  }
  const to = tsSource.indexOf(GENERATED_END, from);
  if (to === -1) throw new Error(`generated end marker #${occurrence} missing`);
  // Line-ending agnostic: Windows checkouts may carry CRLF, while the
  // renderer always emits LF. Compare canonical LF on both sides.
  return tsSource
    .slice(from + GENERATED_START.length, to)
    .replaceAll("\r\n", "\n")
    .trim();
}

/** Splice fresh content into the generated region at index. */
export function spliceGenerated(tsSource: string, occurrence: number, content: string): string {
  let from = -1;
  for (let i = 0; i <= occurrence; i++) {
    from = tsSource.indexOf(GENERATED_START, from + 1);
    if (from === -1) throw new Error(`generated start marker #${occurrence} missing`);
  }
  const to = tsSource.indexOf(GENERATED_END, from);
  if (to === -1) throw new Error(`generated end marker #${occurrence} missing`);
  return tsSource.slice(0, from + GENERATED_START.length) + `\n${content}\n` + tsSource.slice(to);
}
