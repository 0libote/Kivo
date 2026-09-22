import type { AiModelInfo, AiProviderId } from "../types";
import { DEFAULT_AI_MODEL, DEFAULT_AI_PROVIDER } from "../types";

/**
 * Curated suggestions GENERATED from `SUPPORTED_GEMINI_MODELS` in
 * `src-tauri/src/ai/mod.rs` — do not edit by hand, run
 * `bun run generate:models`. The native `list_ai_models` command is the
 * source of truth (dynamic ListModels filtered by the blocklist only); this
 * list is only used in the browser harness and while the native list is
 * loading, so the selector never appears empty.
 *
 * Validation is blocklist-only: any well-formed model id works so newest
 * models keep working without a Kivo update. Only blocked non-text families
 * are rejected — TTS / Live / realtime audio (`tts`, `-live`, `audio`),
 * image (`image`, `banana`), transcription, embedding, video (`veo-`,
 * `omni`), music (`lyria-`), computer-use agents, robotics, and research
 * agents. Mirrors `BLOCKED_MODEL_SUBSTRINGS` in `ai/mod.rs`.
 *
 * Stored as compact rows (one model per line) rather than repeated object
 * literals so the intentional mirror doesn't trip duplication gates; the
 * parity script verifies this section is freshly generated.
 */
// biome-ignore format: generated — `bun run generate:models` owns this block.
// @generated ai-models start
const FALLBACK_ROWS: Array<[id: string, label: string, description: string]> = [
  ["gemini-3.8-flash", "Gemini 3.8 Flash", "Default. Fastest frontier text model, tuned for low-latency edits."],
  ["gemini-3.6-flash", "Gemini 3.6 Flash", "Previous-generation Flash balancing speed and multimodal ability."],
  ["gemini-3.5-flash", "Gemini 3.5 Flash", "Stable frontier model for agentic and coding-adjacent rewrites."],
  ["gemini-3.5-flash-lite", "Gemini 3.5 Flash-Lite", "Cost-efficient stable model for high-volume simple tasks."],
  ["gemini-3-flash-preview", "Gemini 3 Flash Preview", "Preview of the Gemini 3 Flash line. May change without notice."],
  ["gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview", "Strongest reasoning in the list. Slower, best for hard rewrites."],
  ["gemini-3.1-flash-lite", "Gemini 3.1 Flash-Lite", "Cost-efficient text model for high-volume simple tasks."],
];
// @generated ai-models end

export const FALLBACK_AI_MODELS: AiModelInfo[] = FALLBACK_ROWS.map(([id, label, description]) => ({
  id,
  label,
  description,
}));

/** GENERATED from `BLOCKED_MODEL_SUBSTRINGS` in `src-tauri/src/ai/mod.rs` — do not edit by hand. */
// biome-ignore format: generated — `bun run generate:models` owns this block.
// @generated ai-models start
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
// @generated ai-models end

export function canonicalAiModelId(id: string): string {
  const trimmed = id.trim();
  return trimmed.startsWith("models/") ? trimmed.slice("models/".length) : trimmed;
}

export function isBlockedAiModelId(id: string): boolean {
  const canonical = canonicalAiModelId(id).toLowerCase();
  return BLOCKED_AI_MODEL_PATTERNS.some((pattern) => canonical.includes(pattern));
}

export function isUsableAiModelId(id: string): boolean {
  const canonical = canonicalAiModelId(id);
  if (canonical.length < 3 || canonical.length > 128) return false;
  if (!canonical.includes("-")) return false;
  if (!/^[a-z0-9._-]+$/.test(canonical)) return false;
  return !isBlockedAiModelId(canonical);
}

export function normalizeAiModel(id: string | null | undefined): string {
  const canonical = canonicalAiModelId(id ?? "");
  return isUsableAiModelId(canonical) ? canonical : DEFAULT_AI_MODEL;
}

export function normalizeAiProvider(value: string | null | undefined): AiProviderId {
  switch ((value ?? "").trim().toLowerCase()) {
    case "zen":
    case "opencode":
    case "opencode-zen":
      return "zen";
    case "go":
    case "opencode-go":
      return "go";
    case "custom":
    case "openai-compatible":
    case "ollama":
    case "local":
      return "custom";
    default:
      return DEFAULT_AI_PROVIDER;
  }
}

/** Strip the `opencode/` / `opencode-go/` config prefix (`opencode/gpt-5.5`). */
export function canonicalOpenCodeModelId(id: string): string {
  const trimmed = id.trim();
  if (trimmed.startsWith("opencode-go/")) return trimmed.slice("opencode-go/".length);
  if (trimmed.startsWith("opencode/")) return trimmed.slice("opencode/".length);
  return trimmed;
}

/**
 * Zen/Go ids mirror `is_usable_opencode_model` in
 * `src-tauri/src/ai/providers.rs`: lowercase, separator required, same
 * non-text blocklist except `omni` (Go serves a text-capable `mimo-v2-omni`).
 */
export function isUsableOpenCodeModelId(id: string): boolean {
  const canonical = canonicalOpenCodeModelId(id);
  if (canonical.length < 3 || canonical.length > 128) return false;
  if (!canonical.includes("-") && !canonical.includes(".")) return false;
  if (!/^[a-z0-9._-]+$/.test(canonical)) return false;
  const lower = canonical.toLowerCase();
  return BLOCKED_AI_MODEL_PATTERNS.filter((pattern) => pattern !== "omni").every(
    (pattern) => !lower.includes(pattern),
  );
}

/** Custom ids mirror `is_usable_custom_model` (Ollama / LM Studio shapes). */
export function isUsableCustomModelId(id: string): boolean {
  const canonical = id.trim();
  if (canonical.length === 0 || canonical.length > 128) return false;
  return /^[A-Za-z0-9._\-:/]+$/.test(canonical);
}

export function isUsableAiModelIdFor(provider: AiProviderId, id: string): boolean {
  switch (provider) {
    case "zen":
    case "go":
      return isUsableOpenCodeModelId(id);
    case "custom":
      return isUsableCustomModelId(id);
    default:
      return isUsableAiModelId(id);
  }
}

export function canonicalAiModelIdFor(provider: AiProviderId, id: string): string {
  switch (provider) {
    case "zen":
    case "go":
      return canonicalOpenCodeModelId(id);
    case "custom":
      return id.trim();
    default:
      return canonicalAiModelId(id);
  }
}

export function normalizeAiModelFor(provider: AiProviderId, id: string | null | undefined): string {
  const canonical = canonicalAiModelIdFor(provider, id ?? "");
  if (isUsableAiModelIdFor(provider, canonical)) return canonical;
  return providerDefaultModel(provider);
}

/** Mirrors `MAX_AI_MODELS` in `src-tauri/src/ai/mod.rs`. */
export const MAX_AI_MODELS = 5;

/**
 * Normalize an ordered failover queue like the Rust backend
 * (`normalize_model_list`): canonicalize, drop unusable and duplicate ids
 * (first occurrence wins), cap the length, fall back to the provider
 * default when nothing usable remains.
 */
export function normalizeAiModelList(provider: AiProviderId, ids: readonly string[]): string[] {
  const seen = new Set<string>();
  const models: string[] = [];
  for (const id of ids) {
    if (models.length >= MAX_AI_MODELS) break;
    const canonical = canonicalAiModelIdFor(provider, id);
    if (canonical === "" || seen.has(canonical) || !isUsableAiModelIdFor(provider, canonical))
      continue;
    seen.add(canonical);
    models.push(canonical);
  }
  if (models.length === 0) models.push(providerDefaultModel(provider));
  return models;
}

/** Migrate a pre-queue primary/backup pair into a queue. */
export function legacyModelList(
  primary: string | null | undefined,
  backup: string | null | undefined,
): string[] {
  const ids: string[] = [];
  if (primary != null && primary.trim() !== "") ids.push(primary);
  if (backup != null && backup.trim() !== "") ids.push(backup);
  return ids;
}

/** Migrate stored settings that predate the queue (single model + backup). */
export function migrateAiModels(stored: {
  aiModels?: unknown;
  aiModel?: unknown;
  aiBackupModel?: unknown;
}): string[] {
  if (Array.isArray(stored.aiModels))
    return stored.aiModels.filter((id): id is string => typeof id === "string");
  const primary = typeof stored.aiModel === "string" ? stored.aiModel : null;
  const backup = typeof stored.aiBackupModel === "string" ? stored.aiBackupModel : null;
  return legacyModelList(primary, backup);
}

/** Mirrors `AiProvider::default_model` in Rust. */
export function providerDefaultModel(provider: AiProviderId): string {
  switch (provider) {
    case "zen":
      return DEFAULT_AI_MODEL;
    case "go":
      return "glm-5.3-flash";
    case "custom":
      return "llama3.1";
    default:
      return DEFAULT_AI_MODEL;
  }
}

/** Mirrors `normalize_base_url` (None = Ollama default applies). */
export function normalizeAiBaseUrl(raw: string | null | undefined): string | null {
  let trimmed = (raw ?? "").trim();
  // Strip trailing slashes without a regex (avoids backtracking hotspots).
  while (trimmed.endsWith("/")) trimmed = trimmed.slice(0, -1);
  if (trimmed === "" || trimmed.length > 512) return null;
  if (/\s/.test(trimmed)) return null;
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) return trimmed;
  return null;
}

// --- Curated Zen / Go / Custom fallbacks -----------------------------------
// Mirrors `ZEN_CURATED` / `GO_CURATED` / `CUSTOM_CURATED` in
// `src-tauri/src/ai/providers.rs` (id, label, blurb, cost, billing). The
// native `list_ai_models` command prices the live gateway listing from the
// same tables; these rows are only the offline fallback, so the selector
// never appears empty and the cost of each choice stays visible.

type PricedRow = [id: string, label: string, blurb: string, cost: string, billing: string];

const ZEN_FALLBACK_ROWS: PricedRow[] = [
  [
    "gemini-3.8-flash",
    "Gemini 3.8 Flash",
    "Default. Same fast model as the Gemini provider.",
    "$1.50 in / $7.50 out per 1M",
    "zen_credits",
  ],
  [
    "glm-5.3-flash",
    "GLM 5.3 Flash",
    "Cheapest pay-as-you-go coding model.",
    "$0.15 in / $0.50 out per 1M",
    "zen_credits",
  ],
  [
    "kimi-k2.7-code",
    "Kimi K2.7 Code",
    "Strong open coding model, good default for Zen.",
    "$0.95 in / $4.00 out per 1M",
    "zen_credits",
  ],
  [
    "deepseek-v4-flash",
    "DeepSeek V4 Flash",
    "Fast budget reasoning for everyday edits.",
    "$0.14 in / $0.28 out per 1M",
    "zen_credits",
  ],
  [
    "qwen3.7-plus",
    "Qwen 3.7 Plus",
    "Balanced quality for longer rewrites.",
    "$0.40 in / $1.60 out per 1M",
    "zen_credits",
  ],
  [
    "minimax-m3",
    "MiniMax M3",
    "Capable all-rounder for writing tasks.",
    "$0.30 in / $1.20 out per 1M",
    "zen_credits",
  ],
  [
    "gpt-5.4-nano",
    "GPT 5.4 Nano",
    "Tiny OpenAI model for quick cleanup.",
    "$0.20 in / $1.25 out per 1M",
    "zen_credits",
  ],
  [
    "gpt-5.6-luna",
    "GPT 5.6 Luna",
    "Efficient OpenAI model with low rates.",
    "$0.20 in / $1.20 out per 1M",
    "zen_credits",
  ],
  [
    "claude-haiku-4-5",
    "Claude Haiku 4.5",
    "Fast Anthropic model for short tasks.",
    "$1.00 in / $5.00 out per 1M",
    "zen_credits",
  ],
  ["big-pickle", "Big Pickle", "Free stealth model, limited time.", "Free", "free"],
];

const GO_FALLBACK_ROWS: PricedRow[] = [
  [
    "glm-5.3-flash",
    "GLM 5.3 Flash",
    "Recommended. Fast, high-throughput model for short writing tasks.",
    "$0.15 in / $0.50 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "qwen3.8-flash",
    "Qwen 3.8 Flash",
    "Fast alternative with a low monthly usage cost.",
    "$0.15 in / $0.47 out per 1M · $30/mo incl.",
    "go_subscription",
  ],
  [
    "deepseek-v4.1-flash",
    "DeepSeek V4.1 Flash",
    "Fast budget alternative for everyday edits.",
    "$0.15 in / $0.60 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "kimi-k2.7-code",
    "Kimi K2.7 Code",
    "Strong coding model; prose edits may take longer.",
    "$0.95 in / $4.00 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "deepseek-v4-flash",
    "DeepSeek V4 Flash",
    "Fast budget reasoning for everyday edits.",
    "$0.15 in / $0.60 out per 1M · $30/mo incl.",
    "go_subscription",
  ],
  [
    "qwen3.7-plus",
    "Qwen 3.7 Plus",
    "Balanced quality for longer rewrites.",
    "$0.40 in / $1.60 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "minimax-m3",
    "MiniMax M3",
    "Capable all-rounder for writing tasks.",
    "$0.30 in / $1.20 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "mimo-v2.5",
    "MiMo V2.5",
    "Very high request allowance per dollar.",
    "$0.14 in / $0.28 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "muse-spark-1.3-contributor",
    "Muse Spark 1.3",
    "Meta contributor tier; trains on prompts.",
    "$0.10 in / $0.20 out per 1M · $60/mo incl.",
    "go_subscription",
  ],
  [
    "gpt-5.6-luna",
    "GPT 5.6 Luna",
    "Efficient OpenAI model on Go.",
    "$0.20 in / $1.20 out per 1M · $15/mo incl.",
    "go_subscription",
  ],
  [
    "grok-4.6",
    "Grok 4.6",
    "xAI flagship, smaller allowance.",
    "$2.00 in / $6.00 out per 1M · $15/mo incl.",
    "go_subscription",
  ],
  ["union-alpha", "Union Alpha", "Free stealth model, limited time.", "Free", "free"],
];

const CUSTOM_FALLBACK_ROWS: PricedRow[] = [
  [
    "llama3.1",
    "Llama 3.1",
    "Ollama default example — `ollama pull llama3.1` first.",
    "Local · free",
    "local",
  ],
  [
    "qwen2.5-coder:7b",
    "Qwen 2.5 Coder 7B",
    "Good local coding model — `ollama pull qwen2.5-coder:7b`.",
    "Local · free",
    "local",
  ],
  [
    "gemma-3n-e4b",
    "Gemma 3n E4B",
    "Small local model, e.g. via LM Studio.",
    "Local · free",
    "local",
  ],
];

function pricedRows(rows: PricedRow[]): AiModelInfo[] {
  return rows.map(([id, label, description, cost, billing]) => ({
    id,
    label,
    description,
    cost,
    billing,
  }));
}

export const ZEN_FALLBACK_MODELS: AiModelInfo[] = pricedRows(ZEN_FALLBACK_ROWS);
export const GO_FALLBACK_MODELS: AiModelInfo[] = pricedRows(GO_FALLBACK_ROWS);
export const CUSTOM_FALLBACK_MODELS: AiModelInfo[] = pricedRows(CUSTOM_FALLBACK_ROWS);

export function fallbackAiModels(provider: AiProviderId): AiModelInfo[] {
  switch (provider) {
    case "zen":
      return ZEN_FALLBACK_MODELS;
    case "go":
      return GO_FALLBACK_MODELS;
    case "custom":
      return CUSTOM_FALLBACK_MODELS;
    default:
      return FALLBACK_AI_MODELS;
  }
}
