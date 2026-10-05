/** Caps mirror `MAX_VOCABULARY_WORDS` / `MAX_VOCABULARY_WORD_CHARS` in `src-tauri/src/config/mod.rs`. */
export const MAX_VOCABULARY_WORDS = 200;
export const MAX_VOCABULARY_WORD_CHARS = 60;

/** Split pasted/typed bulk text into candidate words (commas or new lines). */
export function parseVocabularyInput(raw: string): string[] {
  return raw
    .split(/[,\n]/)
    .map((word) => word.trim().slice(0, MAX_VOCABULARY_WORD_CHARS).trim())
    .filter((word) => word.length > 0);
}

/** Merge candidates into the list, folding case-insensitive duplicates (first spelling wins). */
export function mergeVocabulary(current: string[], candidates: string[]): string[] {
  const seen = new Set(current.map((word) => word.toLowerCase()));
  const next = [...current];
  for (const raw of candidates) {
    if (next.length >= MAX_VOCABULARY_WORDS) break;
    const candidate = raw.trim().slice(0, MAX_VOCABULARY_WORD_CHARS).trim();
    if (candidate.length === 0 || seen.has(candidate.toLowerCase())) continue;
    seen.add(candidate.toLowerCase());
    next.push(candidate);
  }
  return next;
}
