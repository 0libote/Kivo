import { useCallback, useEffect, useState } from "react";
import {
  FALLBACK_AI_MODELS,
  canonicalAiModelId,
  isBlockedAiModelId,
  isUsableAiModelId,
  normalizeAiModel,
} from "../../ai/models";
import { Button } from "../../components/Button";
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

function customIdError(draft: string, excluded: string | null): string | null {
  if (draft.trim() === "") return "Enter a model ID (e.g. gemini-2.5-flash).";
  if (isBlockedAiModelId(draft)) {
    return "That model can't be used for text requests (speech, image, video, or agent models aren't supported).";
  }
  if (!isUsableAiModelId(draft)) return "Enter a model ID (e.g. gemini-2.5-flash).";
  if (canonicalAiModelId(draft) === excluded) return "Backup must differ from the primary model.";
  return null;
}

/**
 * Model selector. Quick picks come from the native `list_ai_models` command
 * (dynamic ListModels filtered by the blocklist only, bundled fallback while
 * loading or offline); any well-formed model id can also be typed via
 * "Custom model ID". Only blocked non-text families (TTS, live/audio,
 * image, transcription, embedding, video, music, computer-use, agents) are
 * rejected, so newest text models keep working without a Kivo update.
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
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);

  const refreshModels = useCallback(async (silent: boolean) => {
    if (!silent) {
      setRefreshing(true);
      setRefreshError(null);
    }
    try {
      const next = await nativeBridge.listAiModels();
      if (next.length > 0) {
        setModels(next);
        setRefreshError(null);
      } else if (!silent) {
        setRefreshError("No models came back. Showing the saved list.");
      }
    } catch {
      // Keep the bundled fallback so the selector never appears empty.
      if (!silent) setRefreshError("Couldn't refresh models. Showing the saved list.");
    } finally {
      if (!silent) setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void refreshModels(true);
  }, [refreshModels]);

  const normalized = value == null ? null : normalizeAiModel(value);
  const excluded = excludeId == null ? null : canonicalAiModelId(excludeId);
  const inList = normalized != null && models.some(model => model.id === normalized);
  const showingCustom = normalized != null && !inList;

  let selectValue = CUSTOM_VALUE;
  if (normalized == null) {
    selectValue = NONE_VALUE;
  } else if (inList) {
    selectValue = normalized;
  }

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

  const draftError = draft == null ? null : customIdError(draft, excluded);

  return (
    <div className="ai-model-select">
      <div className="ai-model-select__row">
        <select
          aria-label={ariaLabel}
          disabled={disabled || refreshing}
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
        <Button
          aria-label={`Refresh ${ariaLabel} list`}
          compact
          disabled={disabled || refreshing}
          icon="refresh"
          onClick={() => void refreshModels(false)}
          title="Refresh model list from the API"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {refreshError ? (
        <span className="ai-model-select__error" role="status">{refreshError}</span>
      ) : null}
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
