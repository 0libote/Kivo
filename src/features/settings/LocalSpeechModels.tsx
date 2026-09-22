import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import {
  type AppSettings,
  type LocalModelProgress,
  type LocalSpeechModelInfo,
  NativeError,
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
    <div title={`${label}: ${clamped}/100`} {...stylex.props(styles.score)}>
      <span {...stylex.props(styles.scoreLabel)}>{label}</span>
      <div data-testid="local-model-bar" {...stylex.props(styles.bar)}>
        <div style={{ width: `${clamped}%` }} {...stylex.props(styles.barFill)} />
      </div>
    </div>
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

  const activeModel =
    settings.localSpeechModel ?? models.find((model) => model.recommended)?.id ?? null;

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
        setError(
          error_ instanceof NativeError ? error_.message : "The model could not be downloaded.",
        );
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
    <div {...stylex.props(styles.root)}>
      <input
        aria-label="Search speech models"
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Search models…"
        spellCheck={false}
        type="search"
        value={query}
        {...stylex.props(styles.search)}
      />
      <div {...stylex.props(styles.list)}>
        {ordered.map((model) => {
          const downloading = progress[model.id];
          const selected = activeModel === model.id && model.downloaded;
          let actions: ReactNode;
          if (downloading && !model.downloaded) {
            actions = (
              <>
                <ProgressBar
                  isLabelHidden
                  label={`Downloading ${model.name}`}
                  max={downloading.total || model.sizeBytes}
                  value={downloading.downloaded}
                  xstyle={styles.download}
                />
                <Button label="Cancel" onClick={() => cancel(model.id)} size="sm" />
              </>
            );
          } else if (model.downloaded) {
            actions = (
              <>
                {!selected ? (
                  <Button
                    isDisabled={busy !== null}
                    label="Use"
                    onClick={() => void save({ localSpeechModel: model.id })}
                    size="sm"
                    variant="primary"
                  />
                ) : null}
                <Button
                  isDisabled={busy !== null}
                  label="Delete"
                  onClick={() => void remove(model.id)}
                  size="sm"
                  variant="destructive"
                />
              </>
            );
          } else {
            actions = (
              <Button
                isDisabled={busy !== null || !loaded}
                label={busy === model.id ? "Starting…" : "Download"}
                onClick={() => void download(model.id)}
                size="sm"
              />
            );
          }
          return (
            <div
              data-selected={selected}
              data-testid="local-model"
              key={model.id}
              {...stylex.props(styles.model, selected && styles.modelSelected)}
            >
              <div {...stylex.props(styles.top)}>
                <div {...stylex.props(styles.labels)}>
                  <span {...stylex.props(styles.name)}>
                    {model.name}
                    {model.recommended ? (
                      <span {...stylex.props(styles.badge)}>Recommended</span>
                    ) : null}
                    {selected ? (
                      <span {...stylex.props(styles.badge, styles.badgeActive)}>Selected</span>
                    ) : null}
                  </span>
                  <span {...stylex.props(styles.meta)}>{model.description}</span>
                </div>
                <div {...stylex.props(styles.scores)}>
                  <ScoreBar label="Accuracy" value={model.accuracy} />
                  <ScoreBar label="Speed" value={model.speed} />
                </div>
              </div>
              <div {...stylex.props(styles.footer)}>
                <span {...stylex.props(styles.tag)}>{model.family}</span>
                <span {...stylex.props(styles.tag)}>{model.parameters}</span>
                <span {...stylex.props(styles.tag)}>{languageLabel(model.languageCount)}</span>
                {model.streaming ? <span {...stylex.props(styles.tag)}>Streaming</span> : null}
                <span {...stylex.props(styles.size)}>{formatBytes(model.sizeBytes)}</span>
                <div {...stylex.props(styles.actions)}>{actions}</div>
              </div>
            </div>
          );
        })}
        {loaded && ordered.length === 0 ? (
          <p {...stylex.props(styles.empty)}>No speech models match that search.</p>
        ) : null}
      </div>
      {error ? <Banner status="error" title={error} /> : null}
      <p {...stylex.props(styles.note)}>
        Models run entirely on this device; audio never leaves it. Accuracy and speed are relative
        scores, and every model is downloaded once.
      </p>
    </div>
  );
}

const styles = stylex.create({
  root: {
    display: "grid",
    gap: "8px",
  },
  search: {
    width: "100%",
    padding: "7px 10px",
    userSelect: "text",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    fontSize: "12.5px",
    ":focus": {
      borderColor: "color-mix(in srgb, var(--color-accent) 55%, transparent)",
      outline: "none",
    },
    "::placeholder": {
      color: "var(--color-text-disabled)",
    },
  },
  list: {
    display: "grid",
    gap: "8px",
  },
  model: {
    display: "flex",
    flexDirection: "column",
    alignItems: "stretch",
    gap: "8px",
    padding: "10px 12px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  modelSelected: {
    borderColor: "var(--color-accent)",
  },
  top: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: "12px",
  },
  labels: {
    display: "grid",
    gap: "3px",
    minWidth: 0,
  },
  name: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "7px",
    fontSize: "13px",
    fontWeight: 620,
  },
  badge: {
    padding: "1px 5px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "9.5px",
    fontWeight: 700,
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "5px",
  },
  badgeActive: {
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
    borderColor: "transparent",
  },
  meta: {
    color: "var(--color-text-secondary)",
    fontSize: "11.5px",
    lineHeight: 1.35,
  },
  scores: {
    display: "grid",
    flexShrink: 0,
    gap: "4px",
  },
  score: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
  },
  scoreLabel: {
    width: "50px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "9.5px",
    fontWeight: 650,
    textAlign: "right",
  },
  bar: {
    width: "62px",
    height: "5px",
    overflow: "hidden",
    backgroundColor: "color-mix(in srgb, var(--kivo-text-tertiary) 20%, transparent)",
    borderRadius: "999px",
  },
  barFill: {
    display: "block",
    height: "100%",
    backgroundColor: "var(--color-accent)",
    borderRadius: "999px",
  },
  footer: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "6px",
  },
  tag: {
    padding: "1px 5px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "9.5px",
    fontWeight: 650,
    letterSpacing: "0.05em",
    textTransform: "uppercase",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "5px",
  },
  size: {
    color: "var(--color-text-secondary)",
    fontSize: "11px",
  },
  actions: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    gap: "8px",
    marginInlineStart: "auto",
  },
  download: {
    width: "120px",
  },
  empty: {
    color: "var(--color-text-secondary)",
    fontSize: "12.5px",
  },
  note: {
    margin: "2px 0 14px",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
    lineHeight: 1.4,
  },
});
