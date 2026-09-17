import { useMemo, useState } from "react";
import {
  AI_CUSTOM_VALUE,
  CustomModelEditor,
  optionLabel,
  useAiModelOptions,
} from "./AiModelSelect";
import { Button } from "../../components/Button";
import {
  MAX_AI_MODELS,
  canonicalAiModelIdFor,
  fallbackAiModels,
  normalizeAiModelList,
  providerDefaultModel,
} from "../../ai/models";
import type { AiModelInfo, AiProviderId } from "../../types";

interface ModelQueueEditorProps {
  readonly provider: AiProviderId;
  readonly value: string[];
  readonly onChange: (models: string[]) => void;
  readonly disabled?: boolean;
}

function customOptionText(id: string): string {
  return `Custom: ${id}`;
}

/**
 * Ordered failover queue editor. Each row picks one model (cost shown inline
 * so price and priority can be weighed together); rows can be added (up to
 * MAX_AI_MODELS), removed, and reordered. Requests try the queue top to
 * bottom until one succeeds.
 */
export function ModelQueueEditor({
  provider,
  value,
  onChange,
  disabled,
}: ModelQueueEditorProps) {
  const { models, refreshing, refreshError, refresh } = useAiModelOptions(provider);
  const [customRow, setCustomRow] = useState<number | null>(null);
  const [draft, setDraft] = useState("");

  const queue = useMemo(() => normalizeAiModelList(provider, value), [provider, value]);
  const byId = useMemo(() => new Map(models.map(model => [model.id, model])), [models]);

  function commit(next: string[]) {
    setCustomRow(null);
    onChange(normalizeAiModelList(provider, next));
  }

  function addCandidate(): string | null {
    for (const model of fallbackAiModels(provider)) {
      if (!queue.includes(model.id)) return model.id;
    }
    const fallback = providerDefaultModel(provider);
    return queue.includes(fallback) ? null : fallback;
  }

  const candidate = addCandidate();
  const canAdd = !disabled && !refreshing && queue.length < MAX_AI_MODELS && candidate != null;
  let addText = "+ Add model";
  if (refreshing) {
    addText = "Refreshing…";
  } else if (queue.length >= MAX_AI_MODELS) {
    addText = `Up to ${MAX_AI_MODELS} models`;
  }

  return (
    <div className="ai-model-queue">
      <ol className="ai-model-queue__list">
        {queue.map((id, index) => {
          const selected: AiModelInfo | null = byId.get(id) ?? null;
          const inList = selected != null;
          const others = new Set(queue.filter((_, other) => other !== index));
          return (
            <li className="ai-model-queue__row" key={`${index}:${id}`}>
              <span aria-hidden className="ai-model-queue__position">{index + 1}</span>
              <div className="ai-model-queue__pick">
                <select
                  aria-label={`Model ${index + 1} of ${queue.length}`}
                  disabled={disabled || refreshing}
                  onChange={event => {
                    const next = event.target.value;
                    if (next === AI_CUSTOM_VALUE) {
                      setCustomRow(index);
                      setDraft(inList ? "" : id);
                    } else {
                      const updated = [...queue];
                      updated[index] = next;
                      commit(updated);
                    }
                  }}
                  value={inList ? id : AI_CUSTOM_VALUE}
                >
                  {models.map(model => (
                    <option
                      disabled={others.has(model.id)}
                      key={model.id}
                      value={model.id}
                    >
                      {optionLabel(model, others.has(model.id))}
                    </option>
                  ))}
                  <option value={AI_CUSTOM_VALUE}>
                    {inList ? "Custom model ID…" : customOptionText(id)}
                  </option>
                </select>
                {selected?.cost ? (
                  <span className="ai-model-select__cost">{selected.cost}</span>
                ) : null}
                {customRow === index ? (
                  <CustomModelEditor
                    disabled={disabled}
                    draft={draft}
                    provider={provider}
                    onCancel={() => setCustomRow(null)}
                    onCommit={canonical => {
                      // A duplicate of another row would normalize away and
                      // drop this row surprisingly — keep the old value instead.
                      if (others.has(canonicalAiModelIdFor(provider, canonical))) {
                        setCustomRow(null);
                        return;
                      }
                      const updated = [...queue];
                      updated[index] = canonical;
                      commit(updated);
                    }}
                    onDraftChange={setDraft}
                  />
                ) : null}
              </div>
              <div className="ai-model-queue__actions">
                <Button
                  aria-label={`Move ${selected?.label ?? id} up`}
                  compact
                  disabled={disabled || refreshing || index === 0}
                  onClick={() => {
                    const updated = [...queue];
                    [updated[index - 1], updated[index]] = [updated[index], updated[index - 1]];
                    commit(updated);
                  }}
                >↑</Button>
                <Button
                  aria-label={`Move ${selected?.label ?? id} down`}
                  compact
                  disabled={disabled || refreshing || index === queue.length - 1}
                  onClick={() => {
                    const updated = [...queue];
                    [updated[index], updated[index + 1]] = [updated[index + 1], updated[index]];
                    commit(updated);
                  }}
                >↓</Button>
                <Button
                  aria-label={`Remove ${selected?.label ?? id}`}
                  compact
                  disabled={disabled || refreshing || queue.length <= 1}
                  onClick={() => commit(queue.filter((_, other) => other !== index))}
                  tone="danger"
                >×</Button>
              </div>
            </li>
          );
        })}
      </ol>
      <div className="ai-model-queue__footer">
        <Button
          compact
          disabled={!canAdd}
          onClick={() => {
            if (candidate != null) commit([...queue, candidate]);
          }}
        >
          {addText}
        </Button>
        <Button
          aria-label="Refresh model list from the API"
          compact
          disabled={disabled || refreshing}
          icon="refresh"
          onClick={() => void refresh()}
          title="Refresh model list from the API"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {refreshError ? <output className="ai-model-select__error">{refreshError}</output> : null}
    </div>
  );
}
