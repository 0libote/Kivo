import type { AiModelInfo } from "../types";
import { DEFAULT_AI_MODEL } from "../types";

/**
 * Curated suggestions mirroring `SUPPORTED_GEMINI_MODELS` in
 * `src-tauri/src/ai/mod.rs`. The native `list_ai_models` command is the
 * source of truth; this list is only used in the browser harness and while
 * the native list is loading, so the selector never appears empty.
 *
 * Validation is allow-all: any well-formed `gemini-*` id works so newest
 * models keep working without a Kivo update. Only blocked non-text families
 * are rejected — TTS / Live (`tts`, `-live`), image (`image`, `banana`),
 * transcription, embedding, video (`veo-`), music (`lyria-`), robotics, and
 * research agents. Mirrors `BLOCKED_MODEL_SUBSTRINGS` in `ai/mod.rs`.
 */
export const FALLBACK_AI_MODELS: AiModelInfo[] = [
  {
    id: "gemini-3.8-flash",
    label: "Gemini 3.8 Flash",
    description: "Default. Fastest frontier text model, tuned for low-latency edits.",
  },
  {
    id: "gemini-3.7-flash",
    label: "Gemini 3.7 Flash",
    description: "Frontier speed and quality for everyday writing tasks.",
  },
  {
    id: "gemini-3.5-flash",
    label: "Gemini 3.5 Flash",
    description: "Stable frontier model for agentic and coding-adjacent rewrites.",
  },
  {
    id: "gemini-3-flash-preview",
    label: "Gemini 3 Flash Preview",
    description: "Preview of the Gemini 3 Flash line. May change without notice.",
  },
  {
    id: "gemini-3.1-pro-preview",
    label: "Gemini 3.1 Pro Preview",
    description: "Strongest reasoning in the list. Slower, best for hard rewrites.",
  },
  {
    id: "gemini-2.5-pro",
    label: "Gemini 2.5 Pro",
    description: "Deep reasoning over long or complex selections.",
  },
  {
    id: "gemini-2.5-flash",
    label: "Gemini 2.5 Flash",
    description: "Best price-performance for high-volume, low-latency edits.",
  },
  {
    id: "gemini-2.5-flash-lite",
    label: "Gemini 2.5 Flash-Lite",
    description: "Smallest and cheapest. Good for quick cleanup and short text.",
  },
  {
    id: "gemini-3.1-flash-lite",
    label: "Gemini 3.1 Flash-Lite",
    description: "Cost-efficient text model for high-volume simple tasks.",
  },
];

/** Mirrors `BLOCKED_MODEL_SUBSTRINGS` in `src-tauri/src/ai/mod.rs`. */
export const BLOCKED_AI_MODEL_PATTERNS = [
  "tts",
  "-live",
  "image",
  "banana",
  "transcribe",
  "embed",
  "veo-",
  "lyria-",
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
