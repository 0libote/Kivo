import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
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

/**
 * Shared model list: every selector on the page reads the same module-level
 * cache, so primary + backup (Settings + Onboarding) trigger a single
 * `list_ai_models` call and a manual Refresh updates all of them at once.
 */
let sharedModels: AiModelInfo[] = FALLBACK_AI_MODELS;
let sharedRefreshing = false;
let sharedError: string | null = null;
let sharedLoaded = false;
let sharedInflight: Promise<void> | null = null;
let sharedVersion = 0;
const sharedListeners = new Set<() => void>();

function emitShared() {
  sharedVersion += 1;
  sharedListeners.forEach(listener => listener());
}

function subscribeShared(listener: () => void): () => void {
  sharedListeners.add(listener);
  return () => {
    sharedListeners.delete(listener);
  };
}

function refreshSharedModels(silent: boolean): Promise<void> {
  if (sharedInflight) {
    if (silent) return sharedInflight;
    // A load is already running (e.g. initial mount); ride along but show
    // the spinner on the button that was pressed.
    sharedRefreshing = true;
    emitShared();
    return sharedInflight.then(() => {
      sharedRefreshing = false;
      emitShared();
    });
  }
  if (!silent) {
    sharedRefreshing = true;
    sharedError = null;
    emitShared();
  }
  const task = nativeBridge
    .listAiModels()
    .then(next => {
      if (next.length > 0) {
        sharedModels = next;
        sharedError = null;
      } else if (!silent) {
        sharedError = "No models came back. Showing the saved list.";
      }
      sharedLoaded = true;
    })
    .catch(() => {
      // Keep the bundled fallback so the selector never appears empty, but
      // say so even on the silent first load — otherwise offline users never
      // learn the list is stale.
      sharedLoaded = true;
      sharedError = "Couldn't refresh models. Showing the saved list.";
    })
    .then(() => {
      sharedInflight = null;
      if (!silent) sharedRefreshing = false;
      emitShared();
    });
  sharedInflight = task;
  return task;
}

/** Test-only reset so single-fetch tests start from a clean cache. */
export function __resetSharedAiModelsForTests() {
  sharedModels = FALLBACK_AI_MODELS;
  sharedRefreshing = false;
  sharedError = null;
  sharedLoaded = false;
  sharedInflight = null;
  sharedVersion += 1;
}

function useSharedAiModels() {
  useSyncExternalStore(subscribeShared, () => sharedVersion, () => sharedVersion);
  useEffect(() => {
    if (!sharedLoaded) void refreshSharedModels(true);
  }, []);
  const refresh = useCallback(() => refreshSharedModels(false), []);
  return { models: sharedModels, refreshing: sharedRefreshing, refreshError: sharedError, refresh };
}

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
  if (draft.trim() === "") return "Enter a model ID (e.g. gemini-3.6-flash).";
  if (isBlockedAiModelId(draft)) {
    return "That model can't be used for text requests (speech, image, video, or agent models aren't supported).";
  }
  if (!isUsableAiModelId(draft)) return "Enter a model ID (e.g. gemini-3.6-flash).";
  if (canonicalAiModelId(draft) === excluded) return "Backup must differ from the primary model.";
  return null;
}

function resolveSelectValue(normalized: string | null, inList: boolean): string {
  if (normalized == null) return NONE_VALUE;
  if (inList) return normalized;
  return CUSTOM_VALUE;
}

function findSelectedModel(models: AiModelInfo[], normalized: string | null): AiModelInfo | null {
  if (normalized == null) return null;
  return models.find(model => model.id === normalized) ?? null;
}

function customOptionText(showingCustom: boolean, normalized: string | null): string {
  if (showingCustom && normalized != null) return `Custom: ${normalized}`;
  return "Custom model ID…";
}

/** Case-insensitive id/label filter; the current selection is always kept. */
function filterVisibleModels(
  models: AiModelInfo[],
  query: string,
  selectedId: string | null,
): AiModelInfo[] {
  const q = query.trim().toLowerCase();
  if (q === "") return models;
  const filtered = models.filter(
    model => model.id.toLowerCase().includes(q) || model.label.toLowerCase().includes(q),
  );
  if (selectedId != null && !filtered.some(model => model.id === selectedId)) {
    const current = models.find(model => model.id === selectedId);
    if (current) return [current, ...filtered];
  }
  return filtered;
}

function applySelectChoice(
  next: string,
  selected: { showingCustom: boolean; normalized: string | null },
  emit: (modelId: string | null) => void,
  editDraft: (draft: string | null) => void,
) {
  if (next === NONE_VALUE) {
    editDraft(null);
    emit(null);
  } else if (next === CUSTOM_VALUE) {
    editDraft(selected.showingCustom && selected.normalized != null ? selected.normalized : "");
  } else {
    editDraft(null);
    emit(next);
  }
}

function CustomModelEditor({
  draft,
  excluded,
  disabled,
  onDraftChange,
  onCommit,
  onCancel,
}: {
  readonly draft: string;
  readonly excluded: string | null;
  readonly disabled?: boolean;
  readonly onDraftChange: (value: string) => void;
  readonly onCommit: (canonicalId: string) => void;
  readonly onCancel: () => void;
}) {
  const error = customIdError(draft, excluded);
  return (
    <div className="ai-model-select__custom">
      <input
        aria-label="Custom model ID"
        autoCapitalize="none"
        autoComplete="off"
        autoFocus
        disabled={disabled}
        onBlur={() => {
          if (error == null) onCommit(canonicalAiModelId(draft));
        }}
        onChange={event => onDraftChange(event.target.value)}
        onKeyDown={event => {
          if (event.key === "Enter" && error == null) {
            event.currentTarget.blur();
          }
          if (event.key === "Escape") onCancel();
        }}
        placeholder="gemini-3.6-flash"
        spellCheck={false}
        value={draft}
      />
      {error ? (
        <span className="ai-model-select__error" role="alert">{error}</span>
      ) : (
        <span className="ai-model-select__hint">Press Enter to use this model.</span>
      )}
    </div>
  );
}

/**
 * Model selector. Quick picks come from the shared `list_ai_models` cache
 * (dynamic ListModels filtered by the blocklist only, bundled fallback while
 * loading or offline — one fetch for all instances, Refresh updates every
 * selector at once). The search box filters by id or label; any well-formed
 * model id can also be typed via "Custom model ID". Only blocked non-text
 * families (TTS, live/audio, image, transcription, embedding, video, music,
 * computer-use, agents) are rejected, so newest text models keep working
 * without a Kivo update.
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
  const { models, refreshing, refreshError, refresh } = useSharedAiModels();
  const [draft, setDraft] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const normalized = value == null ? null : normalizeAiModel(value);
  const excluded = excludeId == null ? null : canonicalAiModelId(excludeId);
  const inList = normalized != null && models.some(model => model.id === normalized);
  const showingCustom = normalized != null && !inList;
  const selectValue = resolveSelectValue(normalized, inList);

  // Search filters by id or label; the current selection is always kept
  // visible so filtering never blanks out the chosen value.
  const visibleModels = useMemo(
    () => filterVisibleModels(models, query, normalized),
    [models, query, normalized],
  );

  const selected = findSelectedModel(models, normalized);

  return (
    <div className="ai-model-select">
      <input
        aria-label={`Search ${ariaLabel}`}
        autoComplete="off"
        className="ai-model-select__search"
        disabled={disabled || refreshing}
        onChange={event => setQuery(event.target.value)}
        placeholder="Search models…"
        spellCheck={false}
        type="search"
        value={query}
      />
      <div className="ai-model-select__row">
        <select
          aria-label={ariaLabel}
          disabled={disabled || refreshing}
          id={id}
          onChange={event => applySelectChoice(
            event.target.value,
            { showingCustom, normalized },
            onChange,
            setDraft,
          )}
          value={selectValue}
        >
          {allowNone ? <option value={NONE_VALUE}>{noneLabel}</option> : null}
          {visibleModels.map(model => (
            <option
              disabled={excluded != null && model.id === excluded}
              key={model.id}
              value={model.id}
            >
              {model.label}{excluded != null && model.id === excluded ? " (primary)" : ""}
            </option>
          ))}
          {visibleModels.length === 0 ? (
            <option disabled value="__no-match">No matching models</option>
          ) : null}
          <option value={CUSTOM_VALUE}>{customOptionText(showingCustom, normalized)}</option>
        </select>
        <Button
          aria-label={`Refresh ${ariaLabel} list`}
          compact
          disabled={disabled || refreshing}
          icon="refresh"
          onClick={() => void refresh()}
          title="Refresh model list from the API"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {refreshError ? (
        <output className="ai-model-select__error">{refreshError}</output>
      ) : null}
      {selected ? (
        <span className="ai-model-select__description">{selected.description}</span>
      ) : null}
      {draft != null ? (
        <CustomModelEditor
          disabled={disabled}
          draft={draft}
          excluded={excluded}
          onCancel={() => setDraft(null)}
          onCommit={canonical => {
            setDraft(null);
            onChange(canonical);
          }}
          onDraftChange={setDraft}
        />
      ) : null}
    </div>
  );
}
