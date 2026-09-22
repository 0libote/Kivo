import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { nativeBridge } from "../../platform/native";
import type { AiModelInfo, AppSettings } from "../../types";

interface DictationCleanupModelProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
}

/**
 * Picks the single AI model that cleans up dictated text. Leaving it on
 * "Writing Tools models" keeps the existing behavior (the ordered queue);
 * choosing one pins cleanup to that model, which is handy when you want a
 * faster or cheaper model for dictation than for writing.
 */
export function DictationCleanupModel({ settings, save }: DictationCleanupModelProps) {
  const [models, setModels] = useState<AiModelInfo[]>([]);

  useEffect(() => {
    let active = true;
    void nativeBridge
      .listAiModels(settings.aiProvider)
      .then((next) => {
        if (active) setModels(next);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [settings.aiProvider]);

  const selected = settings.dictationCleanupModel ?? "";
  // Keep a stored choice visible even when the live list is offline or the
  // model is no longer offered, so opening Settings never silently drops it.
  const options =
    models.some((model) => model.id === selected) || selected === ""
      ? models
      : [{ id: selected, label: selected, description: "" }, ...models];

  return (
    <select
      aria-label="Dictation cleanup model"
      onChange={(event) => void save({ dictationCleanupModel: event.target.value || null })}
      value={selected}
      {...stylex.props(styles.select)}
    >
      <option value="">Writing Tools models</option>
      {options.map((model) => (
        <option key={model.id} value={model.id}>
          {model.cost ? `${model.label} — ${model.cost}` : model.label}
        </option>
      ))}
    </select>
  );
}

const styles = stylex.create({
  select: {
    minHeight: "30px",
    paddingBlock: 0,
    paddingInlineStart: "9px",
    paddingInlineEnd: "28px",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    boxShadow: "0 1px 2px rgba(17, 19, 23, 0.024)",
    fontSize: "13px",
    ":focus-visible": {
      outline: "2px solid color-mix(in srgb, var(--color-accent) 72%, transparent)",
      outlineOffset: "1px",
    },
  },
});
