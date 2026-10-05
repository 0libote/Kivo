import { describe, expect, it } from "bun:test";
import {
  formatVocabularyExport,
  MAX_VOCABULARY_WORDS,
  mergeVocabulary,
  normalizeVocabularySetting,
  parseVocabularyInput,
  splitVocabularyItem,
} from "./vocabulary";

describe("custom words parsing", () => {
  it("splits on commas and new lines, trims, and drops empties", () => {
    expect(parseVocabularyInput("Kivo, SOC 2\nSiobhán ,, \n")).toEqual([
      { word: "Kivo", meaning: "" },
      { word: "SOC 2", meaning: "" },
      { word: "Siobhán", meaning: "" },
    ]);
    expect(parseVocabularyInput("   ")).toEqual([]);
  });

  it("reads word | meaning pairs from bulk text", () => {
    expect(splitVocabularyItem("SOC 2 | compliance framework")).toEqual({
      word: "SOC 2",
      meaning: "compliance framework",
    });
    expect(splitVocabularyItem("Kivo")).toEqual({ word: "Kivo", meaning: "" });
    expect(splitVocabularyItem("  |  ")).toBeNull();
  });

  it("truncates over-long words instead of dropping them", () => {
    const [entry] = parseVocabularyInput(`  ${"x".repeat(200)}  `);
    expect(entry.word).toBe("x".repeat(60));
  });

  it("merges without case-insensitive duplicates and fills missing meanings", () => {
    expect(
      mergeVocabulary(
        [{ word: "Kivo", meaning: "" }],
        [
          { word: "kivo", meaning: "our product" },
          { word: "SOC 2", meaning: "" },
        ],
      ),
    ).toEqual([
      { word: "Kivo", meaning: "our product" },
      { word: "SOC 2", meaning: "" },
    ]);
    // A meaning already set is never overwritten by a later duplicate.
    expect(
      mergeVocabulary([{ word: "Kivo", meaning: "first" }], [{ word: "KIVO", meaning: "second" }]),
    ).toEqual([{ word: "Kivo", meaning: "first" }]);
  });

  it("respects the word cap", () => {
    const full = Array.from({ length: MAX_VOCABULARY_WORDS }, (_, n) => ({
      word: `word-${n}`,
      meaning: "",
    }));
    expect(mergeVocabulary(full, [{ word: "one-more", meaning: "" }])).toHaveLength(
      MAX_VOCABULARY_WORDS,
    );
    expect(
      mergeVocabulary(
        [],
        [
          { word: "a", meaning: "" },
          { word: "a", meaning: "" },
          { word: "A ", meaning: "" },
        ],
      ),
    ).toEqual([{ word: "a", meaning: "" }]);
  });

  it("formats exports with meanings and coerces legacy word lists", () => {
    expect(
      formatVocabularyExport([
        { word: "Kivo", meaning: "our product" },
        { word: "SOC 2", meaning: "" },
      ]),
    ).toBe("Kivo | our product\nSOC 2");
    expect(normalizeVocabularySetting(["Kivo", { word: "SOC 2", meaning: "x" }, 42, null])).toEqual(
      [
        { word: "Kivo", meaning: "" },
        { word: "SOC 2", meaning: "x" },
      ],
    );
    expect(normalizeVocabularySetting(undefined)).toEqual([]);
  });
});
