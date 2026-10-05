import type { VocabularyWord } from "../../types";

/** Caps mirror `MAX_VOCABULARY_WORDS` / `MAX_VOCABULARY_WORD_CHARS` / `MAX_VOCABULARY_MEANING_CHARS` in `src-tauri/src/config/mod.rs`. */
export const MAX_VOCABULARY_WORDS = 200;
export const MAX_VOCABULARY_WORD_CHARS = 60;
export const MAX_VOCABULARY_MEANING_CHARS = 200;

/** Separators between a word and its meaning in bulk text (`word | what it means`). */
const MEANING_SEPARATORS = ["|", "—", " - "];

/** Split one bulk item into a word plus optional meaning. */
export function splitVocabularyItem(item: string): VocabularyWord | null {
  let word = item.trim();
  let meaning = "";
  for (const separator of MEANING_SEPARATORS) {
    const at = word.indexOf(separator);
    if (at !== -1) {
      meaning = word.slice(at + separator.length).trim();
      word = word.slice(0, at).trim();
      break;
    }
  }
  word = word.slice(0, MAX_VOCABULARY_WORD_CHARS).trim();
  meaning = meaning.slice(0, MAX_VOCABULARY_MEANING_CHARS).trim();
  if (word.length === 0) return null;
  return { word, meaning };
}

/** Split pasted/typed bulk text into entries (commas or new lines). */
export function parseVocabularyInput(raw: string): VocabularyWord[] {
  const entries: VocabularyWord[] = [];
  for (const item of raw.split(/[,\n]/)) {
    const entry = splitVocabularyItem(item);
    if (entry) entries.push(entry);
  }
  return entries;
}

/** Merge candidates into the list, folding case-insensitive duplicates (first spelling wins, later ones still fill a missing meaning). */
export function mergeVocabulary(
  current: VocabularyWord[],
  candidates: VocabularyWord[],
): VocabularyWord[] {
  const seen = new Map(current.map((entry, index) => [entry.word.toLowerCase(), index]));
  const next = current.map((entry) => ({ word: entry.word, meaning: entry.meaning }));
  for (const raw of candidates) {
    const word = raw.word.trim().slice(0, MAX_VOCABULARY_WORD_CHARS).trim();
    if (word.length === 0) continue;
    const meaning = raw.meaning.trim().slice(0, MAX_VOCABULARY_MEANING_CHARS).trim();
    const at = seen.get(word.toLowerCase());
    if (at !== undefined) {
      if (next[at].meaning === "" && meaning !== "") next[at].meaning = meaning;
      continue;
    }
    if (next.length >= MAX_VOCABULARY_WORDS) break;
    seen.set(word.toLowerCase(), next.length);
    next.push({ word, meaning });
  }
  return next;
}

/** Format entries for export (one per line, `word | meaning` when a meaning exists). */
export function formatVocabularyExport(words: VocabularyWord[]): string {
  return words
    .map((entry) => (entry.meaning ? `${entry.word} | ${entry.meaning}` : entry.word))
    .join("\n");
}

/** Coerce a stored value into entries (early settings files stored bare strings). */
export function normalizeVocabularySetting(value: unknown): VocabularyWord[] {
  if (!Array.isArray(value)) return [];
  return mergeVocabulary(
    [],
    value.flatMap((item): VocabularyWord[] => {
      if (typeof item === "string") {
        const entry = splitVocabularyItem(item);
        return entry ? [entry] : [];
      }
      if (
        item &&
        typeof item === "object" &&
        typeof (item as { word?: unknown }).word === "string"
      ) {
        const entry = item as { word: string; meaning?: unknown };
        const word = entry.word.trim().slice(0, MAX_VOCABULARY_WORD_CHARS).trim();
        if (!word) return [];
        const meaning =
          typeof entry.meaning === "string"
            ? entry.meaning.trim().slice(0, MAX_VOCABULARY_MEANING_CHARS).trim()
            : "";
        return [{ word, meaning }];
      }
      return [];
    }),
  );
}
