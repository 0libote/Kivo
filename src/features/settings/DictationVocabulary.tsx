import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { Icon } from "../../components/Icon";
import type { AppSettings } from "../../types";

interface DictationVocabularyProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
  readonly onNotice: (message: string | null) => void;
}

import { MAX_VOCABULARY_WORDS, mergeVocabulary, parseVocabularyInput } from "./vocabulary";

/**
 * Custom-words editor: names, acronyms, and terms the transcriber should
 * prefer and AI cleanup / Writing Tools must not "correct". Words take
 * effect on the next dictation; nothing else to configure.
 */
export function DictationVocabulary({ settings, save, onNotice }: DictationVocabularyProps) {
  const words = settings.dictationVocabulary ?? [];
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  async function commit(next: string[]) {
    if (next.length === words.length && next.every((word, index) => word === words[index])) return;
    setBusy(true);
    onNotice(null);
    try {
      await save({ dictationVocabulary: next });
    } finally {
      setBusy(false);
    }
  }

  function addDraft() {
    const candidates = parseVocabularyInput(draft);
    if (candidates.length === 0) return;
    const next = mergeVocabulary(words, candidates);
    if (next.length >= MAX_VOCABULARY_WORDS && candidates.length > next.length - words.length) {
      onNotice(`Kept the first ${MAX_VOCABULARY_WORDS} words; the list is full.`);
    }
    setDraft("");
    void commit(next);
  }

  function removeWord(word: string) {
    void commit(words.filter((candidate) => candidate !== word));
  }

  function exportWords() {
    if (words.length === 0) return;
    const blob = new Blob([words.join("\n")], { type: "text/plain" });
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
          aria-label="Add custom words"
          disabled={busy}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") addDraft();
          }}
          placeholder="Add words separated by commas — Kivo, SOC 2, Siobhán"
          value={draft}
          {...stylex.props(styles.input)}
        />
        <Button
          isDisabled={busy || parseVocabularyInput(draft).length === 0}
          label="Add"
          onClick={addDraft}
          size="sm"
          variant="secondary"
        />
      </div>
      {words.length === 0 ? (
        <p {...stylex.props(styles.empty)}>
          No custom words yet. The fifty terms you actually mishear beat five hundred you don't —
          keep the list short so the hint stays strong.
        </p>
      ) : (
        <ul aria-label="Custom words" {...stylex.props(styles.list)}>
          {words.map((word) => (
            <li key={word.toLowerCase()} {...stylex.props(styles.item)}>
              <span {...stylex.props(styles.word)}>{word}</span>
              <button
                aria-label={`Remove ${word}`}
                disabled={busy}
                onClick={() => removeWord(word)}
                type="button"
                {...stylex.props(styles.remove)}
              >
                <Icon name="close" size={12} />
              </button>
            </li>
          ))}
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
    flexGrow: 1,
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
  empty: {
    margin: 0,
    fontSize: "12px",
    color: "var(--color-text-secondary)",
  },
  list: {
    display: "flex",
    flexWrap: "wrap",
    gap: "6px",
    margin: 0,
    padding: 0,
    listStyle: "none",
    maxHeight: "148px",
    overflowY: "auto",
  },
  item: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    paddingBlock: "3px",
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
  word: {
    maxWidth: "220px",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  remove: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "20px",
    height: "20px",
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
