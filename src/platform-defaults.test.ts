import { describe, expect, it } from "vitest";
import {
  FALLBACK_AI_MODELS,
  GO_FALLBACK_MODELS,
  MAX_AI_MODELS,
  ZEN_FALLBACK_MODELS,
  CUSTOM_FALLBACK_MODELS,
  fallbackAiModels,
  isUsableAiModelId,
  isUsableAiModelIdFor,
  isUsableCustomModelId,
  isUsableOpenCodeModelId,
  legacyModelList,
  migrateAiModels,
  normalizeAiBaseUrl,
  normalizeAiModel,
  normalizeAiModelFor,
  normalizeAiModelList,
  normalizeAiProvider,
  providerDefaultModel,
} from "./ai/models";
import { formatShortcut } from "./components/ShortcutRecorder";
import { DEFAULT_AI_MODEL, DEFAULT_AI_PROVIDER, defaultSettings } from "./types";

// Cross-platform default parity. The Rust side owns the same defaults
// (ShortcutBinding::dictation_default / writing_tools_default in
// src-tauri/src/config/mod.rs, normalized to Control+Super in shell.rs).
// These tests run on every OS in CI, so a default changed on one platform
// without its counterpart fails fast instead of surfacing weeks later.
describe("platform defaults parity", () => {
  it("uses the native hold shortcut per platform", () => {
    expect(defaultSettings("macos").dictationShortcut).toBe("Fn");
    expect(defaultSettings("windows").dictationShortcut).toBe("Ctrl+Meta");
    expect(defaultSettings("linux").dictationShortcut).toBe("Control+Alt+Space");
  });

  it("uses the portable writing shortcut per platform", () => {
    expect(defaultSettings("macos").writingShortcut).toBe("Ctrl+Shift+Space");
    expect(defaultSettings("windows").writingShortcut).toBe("Ctrl+Space");
    expect(defaultSettings("linux").writingShortcut).toBe("Ctrl+Space");
  });

  it("labels the Windows key as Win, not Meta", () => {
    expect(formatShortcut("Ctrl+Meta", "windows")).toContain("Win");
    expect(formatShortcut("Ctrl+Meta", "windows")).not.toContain("Meta");
  });

  it("renders macOS modifiers as symbols", () => {
    expect(formatShortcut("Ctrl+Shift+Space", "macos")).toEqual(["⌃", "⇧", "Space"]);
  });

  it("uses the same default AI model queue on both platforms", () => {
    expect(DEFAULT_AI_MODEL).toBe("gemini-3.8-flash");
    expect(DEFAULT_AI_PROVIDER).toBe("gemini");
    expect(MAX_AI_MODELS).toBe(5);
    expect(defaultSettings("macos").aiModels).toEqual([DEFAULT_AI_MODEL]);
    expect(defaultSettings("windows").aiModels).toEqual([DEFAULT_AI_MODEL]);
    expect(defaultSettings("linux").aiModels).toEqual([DEFAULT_AI_MODEL]);
    expect(defaultSettings("macos").aiProvider).toBe("gemini");
    expect(defaultSettings("windows").aiProvider).toBe("gemini");
    expect(defaultSettings("linux").aiProvider).toBe("gemini");
    expect(defaultSettings("macos").aiCustomBaseUrl).toBeNull();
  });

  it("only lists text models that work with Kivo", () => {
    expect(FALLBACK_AI_MODELS.length).toBeGreaterThan(0);
    expect(FALLBACK_AI_MODELS.some(model => model.id === DEFAULT_AI_MODEL)).toBe(true);
    for (const model of FALLBACK_AI_MODELS) {
      expect(model.id).not.toMatch(/tts|live|audio|image|banana|transcribe|embed|omni|computer-use/i);
      expect(model.id).not.toMatch(/^veo|^lyria/);
      expect(model.id).not.toContain("deep-research");
      expect(model.id).not.toContain("robotics");
    }
    for (const excluded of [
      "gemini-2.5-flash-preview-tts",
      "gemini-2.5-flash-image",
      "gemini-2.5-flash-live",
      "gemini-2.5-flash-native-audio-preview-12-2025",
      "gemini-2.5-computer-use-preview-10-2025",
      "gemini-omni-1.1-flash",
      "veo-3.1-preview",
      "lyria-3-pro-preview",
    ]) {
      expect(FALLBACK_AI_MODELS.some(model => model.id === excluded)).toBe(false);
      expect(normalizeAiModel(excluded)).toBe(DEFAULT_AI_MODEL);
    }
    expect(normalizeAiModel("  gemini-2.5-flash  ")).toBe("gemini-2.5-flash");
  });

  it("allows newest text models and custom IDs, blocking only non-text families", () => {
    expect(isUsableAiModelId("gemini-4.0-flash")).toBe(true);
    expect(isUsableAiModelId("gemma-3-27b-it")).toBe(true);
    expect(isUsableAiModelId("learnlm-2.0-flash")).toBe(true);
    expect(normalizeAiModel("gemini-4.0-flash")).toBe("gemini-4.0-flash");
    expect(normalizeAiModel("models/gemini-2.5-flash")).toBe("gemini-2.5-flash");
    expect(normalizeAiModel("models/gemma-3-27b-it")).toBe("gemma-3-27b-it");
    expect(isUsableAiModelId("gemini-2.5-flash-preview-tts")).toBe(false);
    expect(isUsableAiModelId("has spaces!")).toBe(false);
    expect(isUsableAiModelId("gemini")).toBe(false);
  });

  it("parses provider ids with a Gemini fallback", () => {
    expect(normalizeAiProvider("zen")).toBe("zen");
    expect(normalizeAiProvider("opencode")).toBe("zen");
    expect(normalizeAiProvider("go")).toBe("go");
    expect(normalizeAiProvider("opencode-go")).toBe("go");
    expect(normalizeAiProvider("custom")).toBe("custom");
    expect(normalizeAiProvider("ollama")).toBe("custom");
    expect(normalizeAiProvider("")).toBe("gemini");
    expect(normalizeAiProvider("unknown")).toBe("gemini");
    expect(providerDefaultModel("gemini")).toBe(DEFAULT_AI_MODEL);
    expect(providerDefaultModel("zen")).toBe(DEFAULT_AI_MODEL);
    expect(providerDefaultModel("go")).toBe("glm-5.3-flash");
    expect(providerDefaultModel("custom")).toBe("llama3.1");
  });

  it("validates Zen/Go ids like the Rust gateway rules", () => {
    expect(isUsableOpenCodeModelId("kimi-k2.7-code")).toBe(true);
    expect(isUsableOpenCodeModelId("qwen3.7-plus")).toBe(true);
    expect(isUsableOpenCodeModelId("opencode/gpt-5.5")).toBe(true);
    expect(isUsableOpenCodeModelId("opencode-go/kimi-k3")).toBe(true);
    // mimo-v2-omni is served by Go: the omni exception keeps it usable.
    expect(isUsableOpenCodeModelId("mimo-v2-omni")).toBe(true);
    expect(isUsableOpenCodeModelId("gemini-2.5-flash-preview-tts")).toBe(false);
    expect(isUsableOpenCodeModelId("gpt")).toBe(false);
    expect(isUsableOpenCodeModelId("has spaces!")).toBe(false);
    expect(normalizeAiModelFor("zen", "opencode/gpt-5.5")).toBe("gpt-5.5");
    expect(normalizeAiModelFor("go", "bogus!!")).toBe("glm-5.3-flash");
  });

  it("validates custom ids like local servers do", () => {
    expect(isUsableCustomModelId("llama3.1")).toBe(true);
    expect(isUsableCustomModelId("llama2")).toBe(true);
    expect(isUsableCustomModelId("google/gemma-3n-e4b")).toBe(true);
    expect(isUsableCustomModelId("qwen2.5-coder:7b")).toBe(true);
    expect(isUsableCustomModelId("")).toBe(false);
    expect(isUsableCustomModelId("has spaces")).toBe(false);
    expect(isUsableAiModelIdFor("custom", "llama3.1")).toBe(true);
    expect(isUsableAiModelIdFor("custom", "llama2")).toBe(true);
    expect(isUsableAiModelIdFor("zen", "llama2")).toBe(false);
  });

  it("normalizes custom base URLs safely", () => {
    expect(normalizeAiBaseUrl(null)).toBeNull();
    expect(normalizeAiBaseUrl("  ")).toBeNull();
    expect(normalizeAiBaseUrl("http://localhost:11434/v1/")).toBe("http://localhost:11434/v1");
    expect(normalizeAiBaseUrl("notaurl")).toBeNull();
    expect(normalizeAiBaseUrl("ftp://host/v1")).toBeNull();
  });

  it("normalizes failover queues like the Rust backend", () => {
    expect(normalizeAiModelList("gemini", ["  gemini-2.5-flash  ", "models/gemini-4.0-flash"])).toEqual([
      "gemini-2.5-flash",
      "gemini-4.0-flash",
    ]);
    // Duplicates collapse, junk drops out, emptied queues fall back.
    expect(normalizeAiModelList("gemini", ["gemini-2.5-flash", "gemini-2.5-flash", "has spaces!", "gemini-2.5-flash-preview-tts"])).toEqual([
      "gemini-2.5-flash",
    ]);
    expect(normalizeAiModelList("gemini", [])).toEqual([DEFAULT_AI_MODEL]);
    // Long queues truncate to the cap.
    const many = Array.from({ length: MAX_AI_MODELS + 3 }, (_, n) => `gemini-test-${n}-flash`);
    const truncated = normalizeAiModelList("gemini", many);
    expect(truncated).toHaveLength(MAX_AI_MODELS);
    expect(truncated[0]).toBe("gemini-test-0-flash");
    // OpenCode prefixes canonicalize inside queues too.
    expect(normalizeAiModelList("zen", ["opencode/kimi-k2.7-code", "glm-5.3-flash"])).toEqual([
      "kimi-k2.7-code",
      "glm-5.3-flash",
    ]);
  });

  it("migrates pre-queue primary/backup pairs", () => {
    expect(legacyModelList("gemini-2.5-flash", "gemini-2.5-flash-lite")).toEqual([
      "gemini-2.5-flash",
      "gemini-2.5-flash-lite",
    ]);
    expect(legacyModelList("gemini-2.5-flash", null)).toEqual(["gemini-2.5-flash"]);
    expect(migrateAiModels({ aiModels: ["a-1", "b-2"] })).toEqual(["a-1", "b-2"]);
    expect(migrateAiModels({ aiModel: "gemini-2.5-flash", aiBackupModel: "gemini-2.5-flash-lite" })).toEqual([
      "gemini-2.5-flash",
      "gemini-2.5-flash-lite",
    ]);
    expect(migrateAiModels({})).toEqual([]);
  });

  it("shows a cost for every Zen/Go/Custom fallback model", () => {
    expect(fallbackAiModels("gemini")).toBe(FALLBACK_AI_MODELS);
    for (const [provider, rows] of [["zen", ZEN_FALLBACK_MODELS], ["go", GO_FALLBACK_MODELS], ["custom", CUSTOM_FALLBACK_MODELS]] as const) {
      expect(rows.length).toBeGreaterThan(0);
      expect(fallbackAiModels(provider)).toBe(rows);
      for (const model of rows) {
        expect(model.cost).toBeTruthy();
        expect(model.billing).toBeTruthy();
      }
      expect(rows.some(model => model.id === providerDefaultModel(provider))).toBe(true);
    }
    expect(ZEN_FALLBACK_MODELS.find(model => model.id === "glm-5.3-flash")?.cost).toBe("$0.15 in / $0.50 out per 1M");
    expect(GO_FALLBACK_MODELS.find(model => model.id === "glm-5.3-flash")?.cost).toBe("$0.15 in / $0.50 out per 1M · $60/mo incl.");
  });
});
