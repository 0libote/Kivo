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
 * Ordered failover queue editor. Each row is a small card: a priority badge
 * (the first row is the main model, the rest are fallbacks), the model
 * picker, and reorder/remove controls. The picker's blurb and cost render
 * underneath so options can be told apart and compared at a glance.
 * Requests try the queue top to bottom until one succeeds.
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
  let addText = "+ Add fallback";
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
          const priority = index === 0 ? "main model" : `fallback ${index}`;
          return (
            <li className="ai-model-queue__row" key={`${index}:${id}`}>
              <span aria-hidden className="ai-model-queue__position" data-first={index === 0}>{index + 1}</span>
              <div className="ai-model-queue__pick">
                <span className="ai-model-queue__role">{index === 0 ? "Primary" : `Fallback ${index}`}</span>
                <div className="ai-model-queue__select-row">
                  <select
                    aria-label={`Model ${index + 1} of ${queue.length} (${priority})`}
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
                  <div className="ai-model-queue__actions">
                    <Button
                      aria-label={`Move ${selected?.label ?? id} up (now ${priority})`}
                      compact
                      disabled={disabled || refreshing || index === 0}
                      icon="chevron-up"
                      onClick={() => {
                        const updated = [...queue];
                        [updated[index - 1], updated[index]] = [updated[index], updated[index - 1]];
                        commit(updated);
                      }}
                      title="Move up"
                    />
                    <Button
                      aria-label={`Move ${selected?.label ?? id} down (now ${priority})`}
                      compact
                      disabled={disabled || refreshing || index === queue.length - 1}
                      icon="chevron-down"
                      onClick={() => {
                        const updated = [...queue];
                        [updated[index], updated[index + 1]] = [updated[index + 1], updated[index]];
                        commit(updated);
                      }}
                      title="Move down"
                    />
                    <Button
                      aria-label={`Remove ${selected?.label ?? id} (${priority})`}
                      compact
                      disabled={disabled || refreshing || queue.length <= 1}
                      icon="close"
                      onClick={() => commit(queue.filter((_, other) => other !== index))}
                      title="Remove"
                      tone="danger"
                    />
                  </div>
                </div>
                {selected?.description ? (
                  <p className="ai-model-queue__blurb">{selected.description}</p>
                ) : null}
                {selected?.cost ? (
                  <p className="ai-model-queue__cost">{selected.cost}</p>
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
