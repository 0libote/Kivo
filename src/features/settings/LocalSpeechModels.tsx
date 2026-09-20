import { useEffect, useRef, useState, type ReactNode } from "react";
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

/**
 * On-device model manager: download, select, and remove Whisper models. Shown
 * only when the local engine is active, so choosing a model here never leaves
 * the engine and the model out of step.
 */
export function LocalSpeechModels({ settings, save }: LocalSpeechModelsProps) {
  const [models, setModels] = useState<LocalSpeechModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, LocalModelProgress>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
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
      <div className="local-models__list">
        {models.map((model) => {
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
              <div className="local-model__labels">
                <span className="local-model__name">
                  {model.name}
                  {model.recommended ? <span className="local-model__badge">Recommended</span> : null}
                  {selected ? <span className="local-model__badge local-model__badge--active">Selected</span> : null}
                </span>
                <span className="local-model__meta">{formatBytes(model.sizeBytes)} · {model.description}</span>
              </div>
              <div className="local-model__actions">{actions}</div>
            </div>
          );
        })}
        {loaded && models.length === 0 ? <p className="setting-empty">No speech models are available.</p> : null}
      </div>
      {error ? <p className="settings-note" role="alert">{error}</p> : null}
      <p className="settings-note">
        Models run entirely on this device; audio never leaves it. They support many languages and
        are downloaded once.
      </p>
    </div>
  );
}
