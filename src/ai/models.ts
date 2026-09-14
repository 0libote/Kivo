import type { AiModelInfo } from "../types";
import { DEFAULT_AI_MODEL } from "../types";

/**
 * Curated suggestions mirroring `SUPPORTED_GEMINI_MODELS` in
 * `src-tauri/src/ai/mod.rs`. The native `list_ai_models` command is the
 * source of truth (dynamic ListModels filtered by the blocklist only); this
 * list is only used in the browser harness and while the native list is
 * loading, so the selector never appears empty.
 *
 * Validation is blocklist-only: any well-formed `gemini-*` id works so newest
 * models keep working without a Kivo update. Only blocked non-text families
 * are rejected — TTS / Live / realtime audio (`tts`, `-live`, `audio`),
 * image (`image`, `banana`), transcription, embedding, video (`veo-`,
 * `omni`), music (`lyria-`), computer-use agents, robotics, and research
 * agents. Mirrors `BLOCKED_MODEL_SUBSTRINGS` in `ai/mod.rs`.
 *
 * Stored as compact rows (one model per line) rather than repeated object
 * literals so the intentional mirror doesn't trip duplication gates; the
 * parity script compares the id column against the Rust table.
 */
const FALLBACK_ROWS: Array<[id: string, label: string, description: string]> = [
  ["gemini-3.8-flash", "Gemini 3.8 Flash", "Default. Fastest frontier text model, tuned for low-latency edits."],
  ["gemini-3.7-flash", "Gemini 3.7 Flash", "Frontier speed and quality for everyday writing tasks."],
  ["gemini-3.5-flash", "Gemini 3.5 Flash", "Stable frontier model for agentic and coding-adjacent rewrites."],
  ["gemini-3-flash-preview", "Gemini 3 Flash Preview", "Preview of the Gemini 3 Flash line. May change without notice."],
  ["gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview", "Strongest reasoning in the list. Slower, best for hard rewrites."],
  ["gemini-2.5-pro", "Gemini 2.5 Pro", "Deep reasoning over long or complex selections."],
  ["gemini-2.5-flash", "Gemini 2.5 Flash", "Best price-performance for high-volume, low-latency edits."],
  ["gemini-2.5-flash-lite", "Gemini 2.5 Flash-Lite", "Smallest and cheapest. Good for quick cleanup and short text."],
  ["gemini-3.1-flash-lite", "Gemini 3.1 Flash-Lite", "Cost-efficient text model for high-volume simple tasks."],
];

export const FALLBACK_AI_MODELS: AiModelInfo[] = FALLBACK_ROWS.map(
  ([id, label, description]) => ({ id, label, description }),
);

/** Mirrors `BLOCKED_MODEL_SUBSTRINGS` in `src-tauri/src/ai/mod.rs`. */
export const BLOCKED_AI_MODEL_PATTERNS = [
  "tts",
  "-live",
  "audio",
  "image",
  "banana",
  "transcribe",
  "embed",
  "veo-",
  "omni",
  "lyria-",
  "computer-use",
  "robotics",
  "deep-research",
] as const;

export function canonicalAiModelId(id: string): string {
  const trimmed = id.trim();
  return trimmed.startsWith("models/") ? trimmed.slice("models/".length) : trimmed;
}

export function isBlockedAiModelId(id: string): boolean {
  const canonical = canonicalAiModelId(id).toLowerCase();
  return BLOCKED_AI_MODEL_PATTERNS.some(pattern => canonical.includes(pattern));
}

export function isUsableAiModelId(id: string): boolean {
  const canonical = canonicalAiModelId(id);
  if (canonical.length < 3 || canonical.length > 128) return false;
  if (!canonical.startsWith("gemini-")) return false;
  if (!/^[a-z0-9.\-]+$/.test(canonical)) return false;
  return !isBlockedAiModelId(canonical);
}

export function isKnownAiModel(id: string): boolean {
  return FALLBACK_AI_MODELS.some(model => model.id === canonicalAiModelId(id));
}

export function normalizeAiModel(id: string | null | undefined): string {
  const canonical = canonicalAiModelId(id ?? "");
  return isUsableAiModelId(canonical) ? canonical : DEFAULT_AI_MODEL;
}

export function normalizeBackupAiModel(
  id: string | null | undefined,
  primary: string,
): string | null {
  if (id == null) return null;
  const canonical = canonicalAiModelId(id);
  if (canonical === "") return null;
  if (!isUsableAiModelId(canonical)) return null;
  if (canonical === canonicalAiModelId(primary)) return null;
  return canonical;
}

export function aiModelLabel(id: string, models: AiModelInfo[] = FALLBACK_AI_MODELS): string {
  return models.find(model => model.id === canonicalAiModelId(id))?.label ?? canonicalAiModelId(id);
}
