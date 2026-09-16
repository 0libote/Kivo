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
      // Keep the bundled fallback so the selector never appears empty.
      sharedLoaded = true;
      if (!silent) sharedError = "Couldn't refresh models. Showing the saved list.";
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
  const { models, refreshing, refreshError, refresh } = useSharedAiModels();
  const [draft, setDraft] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const normalized = value == null ? null : normalizeAiModel(value);
  const excluded = excludeId == null ? null : canonicalAiModelId(excludeId);
  const inList = normalized != null && models.some(model => model.id === normalized);
  const showingCustom = normalized != null && !inList;

  // Search filters by id or label; the current selection is always kept
  // visible so filtering never blanks out the chosen value.
  const visibleModels = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return models;
    const filtered = models.filter(
      model => model.id.toLowerCase().includes(q) || model.label.toLowerCase().includes(q),
    );
    if (normalized != null && !filtered.some(model => model.id === normalized)) {
      const current = models.find(model => model.id === normalized);
      if (current) return [current, ...filtered];
    }
    return filtered;
  }, [models, query, normalized]);

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
          onClick={() => void refresh()}
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
