import { describe, expect, it } from "bun:test";
import { MAX_VOCABULARY_WORDS, mergeVocabulary, parseVocabularyInput } from "./vocabulary";

describe("custom words parsing", () => {
  it("splits on commas and new lines, trims, and drops empties", () => {
    expect(parseVocabularyInput("Kivo, SOC 2\nSiobhán ,, \n")).toEqual([
      "Kivo",
      "SOC 2",
      "Siobhán",
    ]);
    expect(parseVocabularyInput("   ")).toEqual([]);
  });

  it("truncates over-long words instead of dropping them", () => {
    const [word] = parseVocabularyInput(`  ${"x".repeat(200)}  `);
    expect(word).toBe("x".repeat(60));
  });

  it("merges without case-insensitive duplicates and respects the cap", () => {
    expect(mergeVocabulary(["Kivo"], ["kivo", "SOC 2"])).toEqual(["Kivo", "SOC 2"]);
    const full = Array.from({ length: MAX_VOCABULARY_WORDS }, (_, n) => `word-${n}`);
    expect(mergeVocabulary(full, ["one-more"])).toHaveLength(MAX_VOCABULARY_WORDS);
    expect(mergeVocabulary([], ["a", "a", "A "])).toEqual(["a"]);
  });
});
