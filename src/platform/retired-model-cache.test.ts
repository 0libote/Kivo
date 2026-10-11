import { expect, test } from "bun:test";
import { MockBridge } from "./mock";
import { clearRetiredModelCaches } from "./retired-model-cache";

test("retired speech cache cleanup preserves unrelated storage", async () => {
  const names = [
    "desert-ant-voz-3.5.0",
    "kivo-voz-state-ear3.5.0",
    "desert-ant-models",
    "documents",
    "kivo-other",
  ];
  const removed: string[] = [];
  await clearRetiredModelCaches({
    keys: async () => names,
    delete: async (name) => {
      removed.push(name);
      return true;
    },
  });
  expect(removed).toEqual(names.slice(0, 3));
});

test("legacy browser engine preferences migrate without losing user choices", async () => {
  const prior = window.localStorage.getItem("kivo-dev-settings");
  try {
    window.localStorage.setItem(
      "kivo-dev-settings",
      JSON.stringify({ speechEngine: "voz", dictationLanguage: "fr", soundFeedback: false }),
    );
    const settings = await new MockBridge().getSettings();
    expect(settings.speechEngine).toBe("system");
    expect(settings.dictationLanguage).toBe("fr");
    expect(settings.soundFeedback).toBe(false);
  } finally {
    if (prior === null) window.localStorage.removeItem("kivo-dev-settings");
    else window.localStorage.setItem("kivo-dev-settings", prior);
  }
});
