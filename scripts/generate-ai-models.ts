/**
 * Regenerate the Gemini catalog mirror in `src/ai/models.ts` from the Rust
 * source of truth (`src-tauri/src/ai/mod.rs`).
 *
 * Run: `bun run generate:models`
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { extractGenerated, renderGeneratedAiModels, spliceGenerated } from "./ai-model-codegen.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const rustPath = join(root, "src-tauri/src/ai/mod.rs");
const tsPath = join(root, "src/ai/models.ts");

const rustSource = readFileSync(rustPath, "utf8");
const tsRaw = readFileSync(tsPath, "utf8");
// Compare canonical LF, but write back in the file's own convention so a
// CRLF checkout is never rewritten with mixed endings.
const crlf = tsRaw.includes("\r\n");
const tsSource = crlf ? tsRaw.replaceAll("\r\n", "\n") : tsRaw;
const toFile = (text: string): string => (crlf ? text.replaceAll("\n", "\r\n") : text);
const generated = renderGeneratedAiModels(rustSource);

let updated = tsSource;
const currentFallback = extractGenerated(updated, 0);
if (currentFallback !== generated.fallbackRows) {
  updated = spliceGenerated(updated, 0, generated.fallbackRows);
  console.info("updated FALLBACK_ROWS from SUPPORTED_GEMINI_MODELS");
} else {
  console.info("FALLBACK_ROWS already fresh");
}
// Occurrence indices shift only if the first splice changed marker text, and
// markers are never rewritten, so index 1 still addresses the blocklist.
const currentBlocked = extractGenerated(updated, 1);
if (currentBlocked !== generated.blockedPatterns) {
  updated = spliceGenerated(updated, 1, generated.blockedPatterns);
  console.info("updated BLOCKED_AI_MODEL_PATTERNS from BLOCKED_MODEL_SUBSTRINGS");
} else {
  console.info("BLOCKED_AI_MODEL_PATTERNS already fresh");
}

if (updated !== tsSource) {
  writeFileSync(tsPath, toFile(updated));
  console.info("wrote src/ai/models.ts");
} else {
  console.info("src/ai/models.ts is up to date");
}
