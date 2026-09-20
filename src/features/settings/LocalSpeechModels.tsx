import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import {
  NativeError,
  type AppSettings,
  type LocalModelProgress,
  type LocalSpeechModelInfo,
} from "../../types";

interface LocalSpeechModelsProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
}

function formatBytes(bytes: number): string {
  const megabytes = bytes / (1024 * 1024);
  if (megabytes < 1024) return `${Math.round(megabytes)} MB`;
  return `${(megabytes / 1024).toFixed(1)} GB`;
}

function languageLabel(count: number): string {
  if (count <= 1) return "1 language";
  return `${count} languages`;
}

function ScoreBar({ label, value }: { readonly label: string; readonly value: number }) {
  const clamped = Math.max(0, Math.min(100, value));
  return (
    <span className="local-model__score" title={`${label}: ${clamped}/100`}>
      <span className="local-model__score-label">{label}</span>
      <span className="local-model__bar">
        <span style={{ width: `${clamped}%` }} />
      </span>
    </span>
  );
}

/**
 * On-device model manager: download, select, and remove speech models. Shown
 * only when the local engine is active, so choosing a model here never leaves
 * the engine and the model out of step. Accuracy and speed bars make the
 * trade-offs between families visible at a glance, matching how the models are
 * rated upstream.
 */
export function LocalSpeechModels({ settings, save }: LocalSpeechModelsProps) {
  const [models, setModels] = useState<LocalSpeechModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, LocalModelProgress>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [query, setQuery] = useState("");
  // A cancel rejects the in-flight download; without this the rejection would
  // surface as a failure the user did not experience.
  const cancelled = useRef(new Set<string>());

  useNativeEvent<LocalSpeechModelInfo[]>("local-models-changed", setModels);
  useNativeEvent<LocalModelProgress>("local-model-progress", (update) => {
    setProgress((current) => ({ ...current, [update.modelId]: update }));
  });

  useEffect(() => {
    let active = true;
    void nativeBridge
      .listLocalSpeechModels()
      .then((next) => {
        if (active) setModels(next);
      })
      .catch(() => setError("Speech models aren’t available right now."))
      .finally(() => setLoaded(true));
    return () => {
      active = false;
    };
  }, []);

  const activeModel = settings.localSpeechModel ?? models.find((model) => model.recommended)?.id ?? null;

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return models;
    return models.filter((model) =>
      `${model.name} ${model.description} ${model.family}`.toLowerCase().includes(needle),
    );
  }, [models, query]);

  // Installed models first, then the recommended pick, then the catalog order.
  const ordered = useMemo(() => {
    const rank = (model: LocalSpeechModelInfo): number => {
      if (model.downloaded) return 0;
      if (model.recommended) return 1;
      return 2;
    };
    return filtered
      .map((model, index) => ({ model, index }))
      .sort((a, b) => rank(a.model) - rank(b.model) || a.index - b.index)
      .map((entry) => entry.model);
  }, [filtered]);

  function clearProgress(modelId: string) {
    setProgress((current) => {
      const next = { ...current };
      delete next[modelId];
      return next;
    });
  }

  async function download(modelId: string) {
    setBusy(modelId);
    setError(null);
    try {
      setModels(await nativeBridge.downloadLocalSpeechModel(modelId));
      await save({ localSpeechModel: modelId });
    } catch (error_) {
      if (!cancelled.current.has(modelId)) {
        setError(error_ instanceof NativeError ? error_.message : "The model could not be downloaded.");
      }
    } finally {
      cancelled.current.delete(modelId);
      clearProgress(modelId);
      setBusy(null);
    }
  }

  function cancel(modelId: string) {
    cancelled.current.add(modelId);
    clearProgress(modelId);
    void nativeBridge.cancelLocalSpeechModelDownload(modelId).catch(() => {});
  }

  async function remove(modelId: string) {
    setBusy(modelId);
    setError(null);
    try {
      setModels(await nativeBridge.deleteLocalSpeechModel(modelId));
      if ((settings.localSpeechModel ?? null) === modelId) {
        await save({ localSpeechModel: null });
      }
    } catch (error_) {
      setError(error_ instanceof NativeError ? error_.message : "The model could not be removed.");
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="local-models">
      <input
        aria-label="Search speech models"
        className="local-models__search"
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Search models…"
        spellCheck={false}
        type="search"
        value={query}
      />
      <div className="local-models__list">
        {ordered.map((model) => {
          const downloading = progress[model.id];
          const selected = activeModel === model.id && model.downloaded;
          let actions: ReactNode;
          if (downloading && !model.downloaded) {
            actions = (
              <>
                <progress
                  aria-label={`Downloading ${model.name}`}
                  max={downloading.total || model.sizeBytes}
                  value={downloading.downloaded}
                />
                <Button compact onClick={() => cancel(model.id)}>
                  Cancel
                </Button>
              </>
            );
          } else if (model.downloaded) {
            actions = (
              <>
                {!selected ? (
                  <Button compact disabled={busy !== null} onClick={() => void save({ localSpeechModel: model.id })} tone="primary">
                    Use
                  </Button>
                ) : null}
                <Button compact disabled={busy !== null} onClick={() => void remove(model.id)} tone="danger">
                  Delete
                </Button>
              </>
            );
          } else {
            actions = (
              <Button compact disabled={busy !== null || !loaded} onClick={() => void download(model.id)}>
                {busy === model.id ? "Starting…" : "Download"}
              </Button>
            );
          }
          return (
            <div className="local-model" key={model.id} data-selected={selected}>
              <div className="local-model__top">
                <div className="local-model__labels">
                  <span className="local-model__name">
                    {model.name}
                    {model.recommended ? <span className="local-model__badge">Recommended</span> : null}
                    {selected ? <span className="local-model__badge local-model__badge--active">Selected</span> : null}
                  </span>
                  <span className="local-model__meta">{model.description}</span>
                </div>
                <div className="local-model__scores">
                  <ScoreBar label="Accuracy" value={model.accuracy} />
                  <ScoreBar label="Speed" value={model.speed} />
                </div>
              </div>
              <div className="local-model__footer">
                <span className="local-model__tag">{model.family}</span>
                <span className="local-model__tag">{model.parameters}</span>
                <span className="local-model__tag">{languageLabel(model.languageCount)}</span>
                {model.streaming ? <span className="local-model__tag">Streaming</span> : null}
                <span className="local-model__size">{formatBytes(model.sizeBytes)}</span>
                <div className="local-model__actions">{actions}</div>
              </div>
            </div>
          );
        })}
        {loaded && ordered.length === 0 ? <p className="setting-empty">No speech models match that search.</p> : null}
      </div>
      {error ? <p className="settings-note" role="alert">{error}</p> : null}
      <p className="settings-note">
        Models run entirely on this device; audio never leaves it. Accuracy and speed are relative
        scores, and every model is downloaded once.
      </p>
    </div>
  );
}
