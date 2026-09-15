import { describe, expect, it } from "vitest";
import {
  FALLBACK_AI_MODELS,
  isUsableAiModelId,
  normalizeAiModel,
  normalizeBackupAiModel,
} from "./ai/models";
import { formatShortcut } from "./components/ShortcutRecorder";
import { DEFAULT_AI_MODEL, defaultSettings } from "./types";

// Cross-platform default parity. The Rust side owns the same defaults
// (ShortcutBinding::dictation_default / writing_tools_default in
// src-tauri/src/config/mod.rs, normalized to Control+Super in shell.rs).
// These tests run on every OS in CI, so a default changed on one platform
// without its counterpart fails fast instead of surfacing weeks later.
describe("platform defaults parity", () => {
  it("uses the native hold shortcut per platform", () => {
    expect(defaultSettings("macos").dictationShortcut).toBe("Fn");
    expect(defaultSettings("windows").dictationShortcut).toBe("Ctrl+Meta");
  });

  it("uses the portable writing shortcut per platform", () => {
    expect(defaultSettings("macos").writingShortcut).toBe("Ctrl+Shift+Space");
    expect(defaultSettings("windows").writingShortcut).toBe("Ctrl+Space");
  });

  it("labels the Windows key as Win, not Meta", () => {
    expect(formatShortcut("Ctrl+Meta", "windows")).toContain("Win");
    expect(formatShortcut("Ctrl+Meta", "windows")).not.toContain("Meta");
  });

  it("renders macOS modifiers as symbols", () => {
    expect(formatShortcut("Ctrl+Shift+Space", "macos")).toEqual(["⌃", "⇧", "Space"]);
  });

  it("uses the same default AI model on both platforms", () => {
    expect(DEFAULT_AI_MODEL).toBe("gemini-3.8-flash");
    expect(defaultSettings("macos").aiModel).toBe(DEFAULT_AI_MODEL);
    expect(defaultSettings("windows").aiModel).toBe(DEFAULT_AI_MODEL);
    expect(defaultSettings("macos").aiBackupModel).toBeNull();
    expect(defaultSettings("windows").aiBackupModel).toBeNull();
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
    expect(normalizeAiModel("gemini-4.0-flash")).toBe("gemini-4.0-flash");
    expect(normalizeAiModel("models/gemini-2.5-flash")).toBe("gemini-2.5-flash");
    expect(isUsableAiModelId("gemini-2.5-flash-preview-tts")).toBe(false);
    expect(isUsableAiModelId("not-a-model")).toBe(false);
    expect(normalizeBackupAiModel("gemini-2.5-flash", "gemini-3.8-flash")).toBe("gemini-2.5-flash");
    expect(normalizeBackupAiModel(null, "gemini-3.8-flash")).toBeNull();
    expect(normalizeBackupAiModel("", "gemini-3.8-flash")).toBeNull();
    expect(normalizeBackupAiModel("gemini-3.8-flash", "gemini-3.8-flash")).toBeNull();
    expect(normalizeBackupAiModel("gemini-2.5-flash-preview-tts", "gemini-3.8-flash")).toBeNull();
  });
});
