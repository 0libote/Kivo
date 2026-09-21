import { useCallback, useEffect, useSyncExternalStore } from "react";
import {
  canonicalAiModelIdFor,
  fallbackAiModels,
  isUsableAiModelIdFor,
  providerDefaultModel,
} from "../../ai/models";
import { nativeBridge } from "../../platform/native";
import type { AiModelInfo, AiProviderId } from "../../types";

const CUSTOM_VALUE = "__custom";

/**
 * Shared model list, cached per provider: every queue row on the page reads
 * the same module-level cache, so Settings + Onboarding trigger a single
 * `list_ai_models` call per provider and a manual Refresh updates all of
 * them at once. Model discovery names the provider explicitly, so an
 * optimistic provider switch cannot race the native settings write and cache
 * the previous provider's models under the new provider.
 */
interface ProviderCache {
  models: AiModelInfo[];
  refreshing: boolean;
  error: string | null;
  loaded: boolean;
  inflight: Promise<void> | null;
}

const sharedCaches = new Map<AiProviderId, ProviderCache>();

function cacheFor(provider: AiProviderId): ProviderCache {
  let cache = sharedCaches.get(provider);
  if (!cache) {
    cache = { models: fallbackAiModels(provider), refreshing: false, error: null, loaded: false, inflight: null };
    sharedCaches.set(provider, cache);
  }
  return cache;
}

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

function refreshSharedModels(provider: AiProviderId, silent: boolean): Promise<void> {
  const cache = cacheFor(provider);
  if (cache.inflight) {
    if (silent) return cache.inflight;
    // A load is already running (e.g. initial mount); ride along but show
    // the spinner on the button that was pressed.
    cache.refreshing = true;
    emitShared();
    return cache.inflight.then(() => {
      cache.refreshing = false;
      emitShared();
    });
  }
  if (!silent) {
    cache.refreshing = true;
    cache.error = null;
    emitShared();
  }
  const task = nativeBridge
    .listAiModels(provider)
    .then(next => {
      if (next.length > 0) {
        cache.models = next;
        cache.error = null;
      } else if (!silent) {
        cache.error = "No models came back. Showing the saved list.";
      }
      cache.loaded = true;
    })
    .catch(() => {
      // Keep the bundled fallback so the selector never appears empty, but
      // say so even on the silent first load — otherwise offline users never
      // learn the list is stale.
      cache.loaded = true;
      cache.error = "Couldn't refresh models. Showing the saved list.";
    })
    .then(() => {
      cache.inflight = null;
      if (!silent) cache.refreshing = false;
      emitShared();
    });
  cache.inflight = task;
  return task;
}

/** Shared provider model list for pickers (queue rows). */
export function useAiModelOptions(provider: AiProviderId) {
  useSyncExternalStore(subscribeShared, () => sharedVersion, () => sharedVersion);
  useEffect(() => {
    const cache = cacheFor(provider);
    if (!cache.loaded) void refreshSharedModels(provider, true);
  }, [provider]);
  const refresh = useCallback(() => refreshSharedModels(provider, false), [provider]);
  const cache = cacheFor(provider);
  return { models: cache.models, refreshing: cache.refreshing, refreshError: cache.error, refresh };
}

function customIdError(provider: AiProviderId, draft: string): string | null {
  if (draft.trim() === "") return `Enter a model ID (e.g. ${providerDefaultModel(provider)}).`;
  if (!isUsableAiModelIdFor(provider, draft)) {
    return provider === "custom"
      ? "Enter a model ID without spaces (e.g. llama3.1)."
      : "That model can't be used for text requests (speech, image, video, or agent models aren't supported).";
  }
  return null;
}

export const AI_CUSTOM_VALUE = CUSTOM_VALUE;

/** Option text (`Label`, plus ` (in queue)` when taken by another row).
 * Cost and description render under the picker instead, so options stay
 * short and comparable. */
export function optionLabel(model: AiModelInfo, inQueue: boolean): string {
  return inQueue ? `${model.label} (in queue)` : model.label;
}

export function CustomModelEditor({
  draft,
  disabled,
  provider,
  onDraftChange,
  onCommit,
  onCancel,
}: {
  readonly draft: string;
  readonly disabled?: boolean;
  readonly provider: AiProviderId;
  readonly onDraftChange: (value: string) => void;
  readonly onCommit: (canonicalId: string) => void;
  readonly onCancel: () => void;
}) {
  const error = customIdError(provider, draft);
  return (
    <div className="ai-model-select__custom">
      <input
        aria-label="Custom model ID"
        autoCapitalize="none"
        autoComplete="off"
        autoFocus
        disabled={disabled}
        onBlur={() => {
          if (error == null) onCommit(canonicalAiModelIdFor(provider, draft));
        }}
        onChange={event => onDraftChange(event.target.value)}
        onKeyDown={event => {
          if (event.key === "Enter" && error == null) {
            event.currentTarget.blur();
          }
          if (event.key === "Escape") onCancel();
        }}
        placeholder={providerDefaultModel(provider)}
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
