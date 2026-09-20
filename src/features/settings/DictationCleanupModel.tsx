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
      .listAiModels()
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
  const options = models.some((model) => model.id === selected) || selected === ""
    ? models
    : [{ id: selected, label: selected, description: "" }, ...models];

  return (
    <select
      aria-label="Dictation cleanup model"
      onChange={(event) => void save({ dictationCleanupModel: event.target.value || null })}
      value={selected}
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
