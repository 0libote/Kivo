import { useEffect, useState } from "react";
import {
  FALLBACK_AI_MODELS,
  canonicalAiModelId,
  isBlockedAiModelId,
  isUsableAiModelId,
  normalizeAiModel,
} from "../../ai/models";
import { nativeBridge } from "../../platform/native";
import type { AiModelInfo } from "../../types";

const NONE_VALUE = "__none";
const CUSTOM_VALUE = "__custom";

interface AiModelSelectProps {
  /** Current model id, or `null` for "no backup". Never undefined. */
  readonly value: string | null;
  readonly onChange: (modelId: string | null) => void;
  readonly disabled?: boolean;
  readonly ariaLabel?: string;
  readonly id?: string;
  /** Show a "None" option (used by the backup selector). */
  readonly allowNone?: boolean;
  readonly noneLabel?: string;
  /** Option matching this id is disabled (backup can't equal primary). */
  readonly excludeId?: string | null;
}

/**
 * Model selector. Quick picks come from the native `list_ai_models` command
 * (bundled fallback while loading); any well-formed `gemini-*` id can also
 * be typed via "Custom model ID". Only blocked non-text families (TTS, live,
 * image, transcription, embedding, video, music, agents) are rejected, so
 * newest text models keep working without a Kivo update.
 */
export function AiModelSelect({
  value,
  onChange,
  disabled,
  ariaLabel = "AI model",
  id,
  allowNone = false,
  noneLabel = "None (no backup)",
  excludeId = null,
}: AiModelSelectProps) {
  const [models, setModels] = useState<AiModelInfo[]>(FALLBACK_AI_MODELS);
  const [draft, setDraft] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void nativeBridge
      .listAiModels()
      .then(next => {
        if (active && next.length > 0) setModels(next);
      })
      .catch(() => {
        // Keep the bundled fallback so the selector never appears empty.
      });
    return () => {
      active = false;
    };
  }, []);

  const normalized = value == null ? null : normalizeAiModel(value);
  const excluded = excludeId == null ? null : canonicalAiModelId(excludeId);
  const inList = normalized != null && models.some(model => model.id === normalized);
  const showingCustom = normalized != null && !inList;
  const selectValue =
    normalized == null ? NONE_VALUE : inList ? normalized : CUSTOM_VALUE;
  const selected = normalized == null
    ? null
    : (models.find(model => model.id === normalized) ?? null);

  function commitCustom(raw: string) {
    const canonical = canonicalAiModelId(raw);
    if (isUsableAiModelId(canonical) && canonical !== excluded) {
      setDraft(null);
      onChange(canonical);
    }
  }

  const draftError =
    draft == null
      ? null
      : draft.trim() === ""
        ? "Enter a model ID like gemini-2.5-flash."
        : isBlockedAiModelId(draft)
          ? "That model can't be used for text requests (speech, image, video, or agent models aren't supported)."
          : !isUsableAiModelId(draft)
            ? "Enter a model ID like gemini-2.5-flash."
            : canonicalAiModelId(draft) === excluded
              ? "Backup must differ from the primary model."
              : null;

  return (
    <div className="ai-model-select">
      <select
        aria-label={ariaLabel}
        disabled={disabled}
        id={id}
        onChange={event => {
          const next = event.target.value;
          if (next === NONE_VALUE) {
            setDraft(null);
            onChange(null);
          } else if (next === CUSTOM_VALUE) {
            setDraft(showingCustom && normalized != null ? normalized : "");
          } else {
            setDraft(null);
            onChange(next);
          }
        }}
        value={selectValue}
      >
        {allowNone ? <option value={NONE_VALUE}>{noneLabel}</option> : null}
        {models.map(model => (
          <option
            disabled={excluded != null && model.id === excluded}
            key={model.id}
            value={model.id}
          >
            {model.label}{excluded != null && model.id === excluded ? " (primary)" : ""}
          </option>
        ))}
        {showingCustom && normalized != null ? (
          <option value={CUSTOM_VALUE}>Custom: {normalized}</option>
        ) : (
          <option value={CUSTOM_VALUE}>Custom model ID…</option>
        )}
      </select>
      {selected ? (
        <span className="ai-model-select__description">{selected.description}</span>
      ) : null}
      {draft != null ? (
        <div className="ai-model-select__custom">
          <input
            aria-label="Custom model ID"
            autoCapitalize="none"
            autoComplete="off"
            autoFocus
            disabled={disabled}
            onBlur={() => {
              if (draftError == null) commitCustom(draft);
            }}
            onChange={event => setDraft(event.target.value)}
            onKeyDown={event => {
              if (event.key === "Enter" && draftError == null) {
                event.currentTarget.blur();
              }
              if (event.key === "Escape") setDraft(null);
            }}
            placeholder="gemini-2.5-flash"
            spellCheck={false}
            value={draft}
          />
          {draftError ? (
            <span className="ai-model-select__error" role="alert">{draftError}</span>
          ) : (
            <span className="ai-model-select__hint">Press Enter to use this model.</span>
          )}
        </div>
      ) : null}
    </div>
  );
}
