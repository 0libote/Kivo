import type { AiProviderInfo } from "../types";

/**
 * Offline provider metadata used before the native side answers
 * `listAiProviders()`. The mock bridge serves the same table, so it lives
 * here once instead of being copied into both.
 */
export const FALLBACK_AI_PROVIDERS: AiProviderInfo[] = [
  {
    id: "gemini",
    label: "Gemini",
    keyUrl: "https://aistudio.google.com/app/apikey",
    keyOptional: false,
    defaultModel: "gemini-3.8-flash",
    defaultBaseUrl: null,
    supportsLinkSummary: true,
    testUsesQuota: true,
  },
  {
    id: "zen",
    label: "OpenCode Zen",
    keyUrl: "https://opencode.ai/auth",
    keyOptional: false,
    defaultModel: "gemini-3.8-flash",
    defaultBaseUrl: null,
    supportsLinkSummary: false,
    testUsesQuota: true,
  },
  {
    id: "go",
    label: "OpenCode Go",
    keyUrl: "https://opencode.ai/auth",
    keyOptional: false,
    defaultModel: "glm-5.3-flash",
    defaultBaseUrl: null,
    supportsLinkSummary: false,
    testUsesQuota: true,
  },
  {
    id: "custom",
    label: "Custom (OpenAI-compatible)",
    keyUrl: null,
    keyOptional: true,
    defaultModel: "llama3.1",
    defaultBaseUrl: "http://localhost:11434/v1",
    supportsLinkSummary: false,
    testUsesQuota: true,
  },
];
