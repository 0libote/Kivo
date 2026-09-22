import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import * as stylex from "@stylexjs/stylex";
import { useMemo, useState } from "react";
import {
  canonicalAiModelIdFor,
  fallbackAiModels,
  MAX_AI_MODELS,
  normalizeAiModelList,
  providerDefaultModel,
} from "../../ai/models";
import { Icon } from "../../components/Icon";
import type { AiModelInfo, AiProviderId } from "../../types";
import {
  AI_CUSTOM_VALUE,
  CustomModelEditor,
  optionLabel,
  useAiModelOptions,
} from "./AiModelSelect";

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
export function ModelQueueEditor({ provider, value, onChange, disabled }: ModelQueueEditorProps) {
  const { models, refreshing, refreshError, refresh } = useAiModelOptions(provider);
  const [customRow, setCustomRow] = useState<number | null>(null);
  const [draft, setDraft] = useState("");

  const queue = useMemo(() => normalizeAiModelList(provider, value), [provider, value]);
  const byId = useMemo(() => new Map(models.map((model) => [model.id, model])), [models]);
  const costProps = stylex.props(styles.cost);

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
    <div {...stylex.props(styles.root)}>
      <ol {...stylex.props(styles.list)}>
        {queue.map((id, index) => {
          const selected: AiModelInfo | null = byId.get(id) ?? null;
          const inList = selected != null;
          const others = new Set(queue.filter((_, other) => other !== index));
          const priority = index === 0 ? "main model" : `fallback ${index}`;
          return (
            <li key={`${index}:${id}`} {...stylex.props(styles.row)}>
              <span
                aria-hidden
                {...stylex.props(styles.position, index === 0 && styles.positionFirst)}
              >
                {index + 1}
              </span>
              <div {...stylex.props(styles.pick)}>
                <span {...stylex.props(styles.role)}>
                  {index === 0 ? "Primary" : `Fallback ${index}`}
                </span>
                <div {...stylex.props(styles.selectRow)}>
                  <select
                    aria-label={`Model ${index + 1} of ${queue.length} (${priority})`}
                    disabled={disabled || refreshing}
                    onChange={(event) => {
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
                    {...stylex.props(styles.select)}
                  >
                    {models.map((model) => (
                      <option disabled={others.has(model.id)} key={model.id} value={model.id}>
                        {optionLabel(model, others.has(model.id))}
                      </option>
                    ))}
                    <option value={AI_CUSTOM_VALUE}>
                      {inList ? "Custom model ID…" : customOptionText(id)}
                    </option>
                  </select>
                  <div {...stylex.props(styles.actions)}>
                    <IconButton
                      icon={<Icon name="chevron-up" size={14} />}
                      isDisabled={disabled || refreshing || index === 0}
                      label={`Move ${selected?.label ?? id} up (now ${priority})`}
                      onClick={() => {
                        const updated = [...queue];
                        [updated[index - 1], updated[index]] = [updated[index], updated[index - 1]];
                        commit(updated);
                      }}
                      size="sm"
                      tooltip="Move up"
                      variant="secondary"
                    />
                    <IconButton
                      icon={<Icon name="chevron-down" size={14} />}
                      isDisabled={disabled || refreshing || index === queue.length - 1}
                      label={`Move ${selected?.label ?? id} down (now ${priority})`}
                      onClick={() => {
                        const updated = [...queue];
                        [updated[index], updated[index + 1]] = [updated[index + 1], updated[index]];
                        commit(updated);
                      }}
                      size="sm"
                      tooltip="Move down"
                      variant="secondary"
                    />
                    <IconButton
                      icon={<Icon name="close" size={14} />}
                      isDisabled={disabled || refreshing || queue.length <= 1}
                      label={`Remove ${selected?.label ?? id} (${priority})`}
                      onClick={() => commit(queue.filter((_, other) => other !== index))}
                      size="sm"
                      tooltip="Remove"
                      variant="destructive"
                    />
                  </div>
                </div>
                {selected?.description ? (
                  <p {...stylex.props(styles.blurb)}>{selected.description}</p>
                ) : null}
                {selected?.cost ? (
                  <p
                    {...costProps}
                    className={`${costProps.className ?? ""} ai-model-queue__cost`.trim()}
                    data-testid="ai-model-cost"
                  >
                    {selected.cost}
                  </p>
                ) : null}
                {customRow === index ? (
                  <CustomModelEditor
                    disabled={disabled}
                    draft={draft}
                    provider={provider}
                    onCancel={() => setCustomRow(null)}
                    onCommit={(canonical) => {
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
      <div {...stylex.props(styles.footer)}>
        <Button
          isDisabled={!canAdd}
          label={addText}
          onClick={() => {
            if (candidate != null) commit([...queue, candidate]);
          }}
          size="sm"
          variant="secondary"
        />
        <Button
          icon={<Icon name="refresh" size={14} />}
          isDisabled={disabled || refreshing}
          label="Refresh model list from the API"
          onClick={() => void refresh()}
          size="sm"
          tooltip="Refresh model list from the API"
          variant="secondary"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {refreshError ? <output {...stylex.props(styles.error)}>{refreshError}</output> : null}
    </div>
  );
}

const styles = stylex.create({
  root: {
    display: "grid",
    gap: "8px",
    width: "100%",
  },
  list: {
    display: "grid",
    gap: "8px",
    margin: 0,
    padding: 0,
    listStyleType: "none",
  },
  row: {
    display: "flex",
    alignItems: "flex-start",
    gap: "10px",
    paddingBlock: "10px",
    paddingInline: "12px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  position: {
    display: "grid",
    placeItems: "center",
    flexShrink: 0,
    width: "26px",
    height: "26px",
    marginTop: "2px",
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--color-background-muted)",
    borderRadius: "50%",
    fontSize: "12px",
    fontWeight: 650,
    fontVariantNumeric: "tabular-nums",
  },
  positionFirst: {
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
  },
  pick: {
    display: "grid",
    flex: 1,
    minWidth: 0,
    alignContent: "start",
    gap: "5px",
  },
  role: {
    color: "var(--kivo-text-tertiary)",
    fontSize: "10px",
    fontWeight: 700,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  selectRow: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
  },
  select: {
    flex: 1,
    minWidth: 0,
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
  actions: {
    display: "flex",
    flexShrink: 0,
    gap: "4px",
  },
  blurb: {
    margin: 0,
    color: "var(--color-text-secondary)",
    fontSize: "12px",
    lineHeight: 1.4,
  },
  cost: {
    margin: 0,
    color: "var(--kivo-text-tertiary)",
    fontSize: "11px",
    fontVariantNumeric: "tabular-nums",
  },
  footer: {
    display: "flex",
    gap: "7px",
  },
  error: {
    color: "var(--color-error)",
    fontSize: "11px",
    lineHeight: 1.35,
  },
});
