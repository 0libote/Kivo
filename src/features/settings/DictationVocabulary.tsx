import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { Icon } from "../../components/Icon";
import type { AppSettings, VocabularyWord } from "../../types";
import {
  formatVocabularyExport,
  MAX_VOCABULARY_WORDS,
  mergeVocabulary,
  normalizeVocabularySetting,
  parseVocabularyInput,
} from "./vocabulary";

interface DictationVocabularyProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
  readonly onNotice: (message: string | null) => void;
}

function wordsEqual(a: VocabularyWord[], b: VocabularyWord[]): boolean {
  return (
    a.length === b.length &&
    a.every((entry, index) => entry.word === b[index].word && entry.meaning === b[index].meaning)
  );
}

/**
 * Custom-words editor: names, acronyms, and terms the transcriber should
 * prefer with your exact spelling, each with optional context ("what it
 * means") that helps the AI use them correctly. Words take effect on the
 * next dictation; nothing else to configure.
 */
export function DictationVocabulary({ settings, save, onNotice }: DictationVocabularyProps) {
  const words = normalizeVocabularySetting(settings.dictationVocabulary);
  const [wordDraft, setWordDraft] = useState("");
  const [meaningDraft, setMeaningDraft] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [editMeaning, setEditMeaning] = useState("");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  async function commit(next: VocabularyWord[]) {
    if (wordsEqual(next, words)) return;
    setBusy(true);
    onNotice(null);
    try {
      await save({ dictationVocabulary: next });
    } finally {
      setBusy(false);
    }
  }

  function addDraft() {
    // The meaning field applies to added words that don't already carry one
    // (`word | meaning` pairs in bulk text keep their own).
    const fallback = meaningDraft.trim();
    const candidates = parseVocabularyInput(wordDraft).map((entry) => {
      if (!fallback || entry.meaning) return entry;
      return { word: entry.word, meaning: fallback };
    });
    if (candidates.length === 0) return;
    const next = mergeVocabulary(words, candidates);
    if (next.length >= MAX_VOCABULARY_WORDS) {
      onNotice(`Kept the first ${MAX_VOCABULARY_WORDS} words; the list is full.`);
    }
    setWordDraft("");
    setMeaningDraft("");
    void commit(next);
  }

  function removeWord(word: string) {
    setEditing((current) => (current === word.toLowerCase() ? null : current));
    void commit(words.filter((entry) => entry.word !== word));
  }

  function saveMeaning(word: string) {
    const next = words.map((entry) => {
      if (entry.word !== word) return entry;
      return { word: entry.word, meaning: editMeaning.trim().slice(0, 200) };
    });
    setEditing(null);
    void commit(next);
  }

  function exportWords() {
    if (words.length === 0) return;
    const blob = new Blob([formatVocabularyExport(words)], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "kivo-vocabulary.txt";
    anchor.click();
    URL.revokeObjectURL(url);
  }

  async function importFile(file: File) {
    const text = await file.text().catch(() => "");
    const next = mergeVocabulary(words, parseVocabularyInput(text));
    if (next.length === words.length) {
      onNotice("No new words found in that file.");
    } else if (next.length >= MAX_VOCABULARY_WORDS) {
      onNotice(`Imported up to the ${MAX_VOCABULARY_WORDS}-word limit.`);
    }
    await commit(next);
  }

  return (
    <div {...stylex.props(styles.wrap)}>
      <div {...stylex.props(styles.addRow)}>
        <input
          aria-label="Word to add"
          disabled={busy}
          onChange={(event) => setWordDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") addDraft();
          }}
          placeholder="Kivo, SOC 2, Siobhán"
          value={wordDraft}
          {...stylex.props(styles.input, styles.wordInput)}
        />
        <input
          aria-label="What it means (optional)"
          disabled={busy}
          onChange={(event) => setMeaningDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") addDraft();
          }}
          placeholder="What it means (optional)"
          value={meaningDraft}
          {...stylex.props(styles.input, styles.meaningInput)}
        />
        <Button
          isDisabled={busy || wordDraft.trim().length === 0}
          label="Add"
          onClick={addDraft}
          size="sm"
          variant="secondary"
        />
      </div>
      {words.length === 0 ? (
        <p {...stylex.props(styles.empty)}>
          No custom words yet. Add the names and terms you mishear most, with a short note on what
          each one means — the AI uses that context to spell and use them correctly. A focused list
          beats a long one: the terms you actually get wrong matter more than hundreds you don't.
        </p>
      ) : (
        <ul aria-label="Custom words" {...stylex.props(styles.list)}>
          {words.map((entry) => {
            const isEditing = editing === entry.word.toLowerCase();
            return (
              <li key={entry.word.toLowerCase()} {...stylex.props(styles.item)}>
                {isEditing ? (
                  <span {...stylex.props(styles.editRow)}>
                    <strong {...stylex.props(styles.word)}>{entry.word}</strong>
                    <input
                      aria-label={`Meaning for ${entry.word}`}
                      disabled={busy}
                      onChange={(event) => setEditMeaning(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") saveMeaning(entry.word);
                        if (event.key === "Escape") setEditing(null);
                      }}
                      placeholder="What it means"
                      // The row mounts from the Edit button's own click (which
                      // unmounts), so focus the input as focus management —
                      // not a page-load autofocus.
                      ref={(input) => {
                        input?.focus();
                      }}
                      value={editMeaning}
                      {...stylex.props(styles.input, styles.inlineInput)}
                    />
                    <button
                      aria-label={`Save meaning for ${entry.word}`}
                      disabled={busy}
                      onClick={() => saveMeaning(entry.word)}
                      type="button"
                      {...stylex.props(styles.iconButton)}
                    >
                      <Icon name="check" size={12} />
                    </button>
                  </span>
                ) : (
                  <span {...stylex.props(styles.entryRow)}>
                    <span {...stylex.props(styles.entryText)}>
                      <strong {...stylex.props(styles.word)}>{entry.word}</strong>
                      {entry.meaning ? (
                        <span {...stylex.props(styles.meaning)}>{entry.meaning}</span>
                      ) : null}
                    </span>
                    <button
                      aria-label={
                        entry.meaning
                          ? `Edit meaning for ${entry.word}`
                          : `Add meaning for ${entry.word}`
                      }
                      disabled={busy}
                      onClick={() => {
                        setEditMeaning(entry.meaning);
                        setEditing(entry.word.toLowerCase());
                      }}
                      type="button"
                      {...stylex.props(styles.iconButton)}
                    >
                      <Icon name="pencil" size={12} />
                    </button>
                    <button
                      aria-label={`Remove ${entry.word}`}
                      disabled={busy}
                      onClick={() => removeWord(entry.word)}
                      type="button"
                      {...stylex.props(styles.iconButton)}
                    >
                      <Icon name="close" size={12} />
                    </button>
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <div {...stylex.props(styles.footer)}>
        <span {...stylex.props(styles.count)}>
          {words.length} of {MAX_VOCABULARY_WORDS} words
        </span>
        <span {...stylex.props(styles.actions)}>
          <Button
            isDisabled={busy}
            label="Import"
            onClick={() => fileRef.current?.click()}
            size="sm"
            variant="ghost"
          />
          <Button
            isDisabled={busy || words.length === 0}
            label="Export"
            onClick={exportWords}
            size="sm"
            variant="ghost"
          />
        </span>
      </div>
      <input
        accept=".txt,.csv,text/plain"
        aria-hidden="true"
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = "";
          if (file) void importFile(file).catch(() => onNotice("That file couldn't be read."));
        }}
        ref={fileRef}
        tabIndex={-1}
        type="file"
        {...stylex.props(styles.hiddenFile)}
      />
    </div>
  );
}

const styles = stylex.create({
  wrap: {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
    width: "100%",
  },
  addRow: {
    display: "flex",
    gap: "8px",
    alignItems: "center",
  },
  input: {
    minHeight: "30px",
    paddingBlock: 0,
    paddingInline: "9px",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    fontSize: "13px",
  },
  wordInput: {
    flexGrow: 2,
    flexBasis: "0%",
    minWidth: "0px",
  },
  meaningInput: {
    flexGrow: 3,
    flexBasis: "0%",
    minWidth: "0px",
  },
  inlineInput: {
    flexGrow: 1,
    minWidth: "0px",
    minHeight: "24px",
    fontSize: "12px",
  },
  empty: {
    margin: 0,
    fontSize: "12px",
    color: "var(--color-text-secondary)",
  },
  list: {
    display: "flex",
    flexDirection: "column",
    gap: "6px",
    margin: 0,
    padding: 0,
    listStyle: "none",
    maxHeight: "168px",
    overflowY: "auto",
  },
  item: {
    paddingBlock: "5px",
    paddingInlineStart: "9px",
    paddingInlineEnd: "5px",
    backgroundColor: "var(--color-background-muted)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    fontSize: "12px",
    color: "var(--color-text-primary)",
  },
  entryRow: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
  },
  editRow: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
  },
  entryText: {
    display: "flex",
    flexDirection: "column",
    flexGrow: 1,
    minWidth: "0px",
  },
  word: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  meaning: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
  },
  iconButton: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "20px",
    height: "20px",
    flexShrink: 0,
    padding: 0,
    border: "none",
    borderRadius: "var(--radius-element)",
    backgroundColor: "transparent",
    color: "var(--color-text-secondary)",
    cursor: "pointer",
  },
  footer: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
  },
  count: {
    fontSize: "12px",
    color: "var(--color-text-secondary)",
  },
  actions: {
    display: "flex",
    gap: "4px",
  },
  hiddenFile: {
    display: "none",
  },
});
