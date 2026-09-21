import { describe, expect, it } from "vitest";
import { defaultSettings } from "../../types";
import type { AppSettings, WritingPreset } from "../../types";
import {
  DEFAULT_PRESET_TEMPLATE,
  INSTRUCTION_PLACEHOLDER,
  MAX_WRITING_PRESETS,
  OUTPUT_RULE_PLACEHOLDER,
  RESULT_OUTPUT_RULE,
  SELECTION_OUTPUT_RULE,
  builtinActionFor,
  defaultWritingPresets,
  isBuiltInPreset,
  newCustomPreset,
  normalizeWritingPreset,
  resolvePresetPrompt,
  resolveWritingPresets,
  withAddedPreset,
  withMovedPreset,
  withRemovedPreset,
  withResetAllPresets,
  withResetPreset,
  withWritingPreset,
} from "./presets";

function base(): AppSettings {
  return { ...defaultSettings("linux") };
}

function proofreadPreset(overrides: Partial<WritingPreset> = {}): WritingPreset {
  return { ...defaultWritingPresets()[0], ...overrides };
}

describe("defaultWritingPresets", () => {
  it("mirrors the eight built-in actions with defaults", () => {
    const presets = defaultWritingPresets();
    expect(presets).toHaveLength(8);
    expect(presets.map((preset) => preset.id)).toContain("proofread");
    expect(presets.find((preset) => preset.id === "summarize")?.replacesSelection).toBe(false);
    expect(presets.every((preset) => preset.template === null && preset.models.length === 0)).toBe(true);
  });

  it("recognises built-ins and maps custom presets to the custom action", () => {
    expect(isBuiltInPreset("proofread")).toBe(true);
    expect(isBuiltInPreset("custom-123")).toBe(false);
    expect(builtinActionFor("summarize")).toBe("summarize");
    expect(builtinActionFor("custom-123")).toBe("custom");
  });
});

describe("resolveWritingPresets", () => {
  it("follows the enabled order and applies stored overrides", () => {
    const settings = base();
    settings.enabledWritingActions = ["summarize", "proofread"];
    settings.writingPresets = [proofreadPreset({ label: "Tidy up" })];
    const resolved = resolveWritingPresets(settings);
    expect(resolved.map((preset) => preset.id)).toEqual(["summarize", "proofread"]);
    expect(resolved[0].label).toBe("Summarize");
    expect(resolved[1].label).toBe("Tidy up");
  });

  it("drops enabled ids that are neither built-in nor defined", () => {
    const settings = base();
    settings.enabledWritingActions = ["proofread", "missing-preset"];
    expect(resolveWritingPresets(settings).map((preset) => preset.id)).toEqual(["proofread"]);
  });
});

describe("normalizeWritingPreset", () => {
  it("requires a usable id", () => {
    expect(normalizeWritingPreset(proofreadPreset({ id: "   " }))).toBeNull();
  });

  it("trims fields, blanks an empty template, and cleans the model list", () => {
    const normalized = normalizeWritingPreset(
      proofreadPreset({
        id: " custom-1 ",
        label: "  Warm  ",
        instruction: "  Say hello  ",
        template: "   ",
        models: [" gemini-3.8-flash ", "  ", "kimi-k2 "],
      }),
    );
    expect(normalized).toMatchObject({
      id: "custom-1",
      label: "Warm",
      instruction: "Say hello",
      template: null,
      models: ["gemini-3.8-flash", "kimi-k2"],
    });
  });

  it("keeps a per-preset model priority", () => {
    const normalized = normalizeWritingPreset(proofreadPreset({ models: ["a", "b"] }));
    expect(normalized?.models).toEqual(["a", "b"]);
  });
});

describe("preset mutations", () => {
  it("upserts a preset without touching the enabled order", () => {
    const settings = base();
    settings.enabledWritingActions = ["proofread"];
    const patch = withWritingPreset(settings, proofreadPreset({ label: "Tidy" }));
    expect(patch.enabledWritingActions).toBeUndefined();
    expect(patch.writingPresets).toHaveLength(1);
    expect(patch.writingPresets?.[0].label).toBe("Tidy");
  });

  it("replaces an existing definition rather than duplicating it", () => {
    const settings = base();
    settings.writingPresets = [proofreadPreset({ label: "First" })];
    const patch = withWritingPreset(settings, proofreadPreset({ label: "Second" }));
    expect(patch.writingPresets).toHaveLength(1);
    expect(patch.writingPresets?.[0].label).toBe("Second");
  });

  it("ignores an unusable preset", () => {
    expect(withWritingPreset(base(), proofreadPreset({ id: "" }))).toEqual({});
  });

  it("adds a custom preset at the end of the enabled order", () => {
    const settings = base();
    const custom = newCustomPreset();
    const patch = withAddedPreset(settings, custom);
    expect(patch.writingPresets).toContainEqual(custom);
    expect(patch.enabledWritingActions?.at(-1)).toBe(custom.id);
    expect(patch.enabledWritingActions).toHaveLength(settings.enabledWritingActions.length + 1);
  });

  it("keeps built-in overrides when removing, but deletes custom definitions", () => {
    const settings = base();
    settings.writingPresets = [proofreadPreset({ label: "Tidy" })];
    const removedBuiltIn = withRemovedPreset(settings, "proofread");
    expect(removedBuiltIn.enabledWritingActions).not.toContain("proofread");
    expect(removedBuiltIn.writingPresets).toBeUndefined();

    const custom = newCustomPreset();
    settings.writingPresets = [custom];
    settings.enabledWritingActions = [...settings.enabledWritingActions, custom.id];
    const removedCustom = withRemovedPreset(settings, custom.id);
    expect(removedCustom.enabledWritingActions).not.toContain(custom.id);
    expect(removedCustom.writingPresets?.some((preset) => preset.id === custom.id)).toBe(false);
  });

  it("moves presets within bounds and ignores no-op moves", () => {
    const settings = base();
    settings.enabledWritingActions = ["proofread", "rewrite", "friendly"];
    expect(withMovedPreset(settings, "proofread", 1).enabledWritingActions).toEqual([
      "rewrite",
      "proofread",
      "friendly",
    ]);
    expect(withMovedPreset(settings, "proofread", -1)).toEqual({});
    expect(withMovedPreset(settings, "friendly", 1)).toEqual({});
    expect(withMovedPreset(settings, "missing", 1)).toEqual({});
  });

  it("resets a single preset and the whole set", () => {
    const settings = base();
    const custom = newCustomPreset();
    settings.writingPresets = [proofreadPreset({ label: "Tidy" }), custom];
    expect(withResetPreset(settings, "proofread").writingPresets).toEqual([custom]);
    expect(withResetPreset(settings, custom.id).writingPresets).toEqual([proofreadPreset({ label: "Tidy" })]);
    expect(withResetAllPresets()).toEqual({
      writingPresets: [],
      enabledWritingActions: defaultWritingPresets().map((preset) => preset.id),
    });
  });
});

describe("resolvePresetPrompt", () => {
  it("substitutes the instruction and the selection output rule", () => {
    const prompt = resolvePresetPrompt(proofreadPreset({ instruction: "Fix it", replacesSelection: true }));
    expect(prompt).toContain("Fix it");
    expect(prompt).toContain(SELECTION_OUTPUT_RULE);
    expect(prompt).not.toContain(INSTRUCTION_PLACEHOLDER);
    expect(prompt).not.toContain(OUTPUT_RULE_PLACEHOLDER);
  });

  it("uses the result output rule for informational presets", () => {
    const prompt = resolvePresetPrompt(proofreadPreset({ instruction: "Summarize", replacesSelection: false }));
    expect(prompt).toContain(RESULT_OUTPUT_RULE);
    expect(prompt).not.toContain(SELECTION_OUTPUT_RULE);
  });

  it("keeps a custom template's remaining placeholders working", () => {
    const prompt = resolvePresetPrompt(
      proofreadPreset({ instruction: "Be kind", template: `Custom ${INSTRUCTION_PLACEHOLDER}` }),
    );
    expect(prompt).toBe("Custom Be kind");
  });

  it("mirrors the backend default template", () => {
    expect(DEFAULT_PRESET_TEMPLATE).toContain(INSTRUCTION_PLACEHOLDER);
    expect(DEFAULT_PRESET_TEMPLATE).toContain(OUTPUT_RULE_PLACEHOLDER);
  });
});

describe("preset cap", () => {
  it("exposes the same cap as the backend", () => {
    expect(MAX_WRITING_PRESETS).toBe(24);
  });
});
