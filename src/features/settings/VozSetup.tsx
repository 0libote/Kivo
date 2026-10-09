import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useState } from "react";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, VozModelProgress, VozModelStatus } from "../../types";

interface VozSetupProps {
  readonly context: AppContext;
  readonly onStatus?: (status: VozModelStatus) => void;
}

const initialStatus: VozModelStatus = {
  supported: false,
  downloaded: false,
  phase: "unsupported",
  progress: null,
  runtime: "Checking availability",
  error: null,
};

export function VozSetup({ context, onStatus }: VozSetupProps) {
  const [status, setStatus] = useState(() => {
    const supported = !nativeBridge.isNative || context.platform !== "linux";
    return {
      ...initialStatus,
      supported,
      phase: supported ? ("notDownloaded" as const) : initialStatus.phase,
    };
  });
  const [busy, setBusy] = useState(false);

  useNativeEvent<VozModelProgress>("voz-model-status", (update) => {
    setStatus((current) => {
      const next = {
        ...current,
        phase: update.phase,
        progress: update.progress,
        error: update.error,
        downloaded: update.phase === "ready" || current.downloaded,
      };
      onStatus?.(next);
      return next;
    });
    if (update.phase === "ready" || update.phase === "failed") setBusy(false);
  });

  useEffect(() => {
    let active = true;
    void nativeBridge
      .getVozModelStatus()
      .then((next) => {
        if (active) {
          setStatus(next);
          onStatus?.(next);
        }
      })
      .catch((cause) => {
        if (!active) return;
        const supported = !nativeBridge.isNative || context.platform === "windows";
        let runtime = initialStatus.runtime;
        if (context.platform === "windows") {
          runtime = "WebView2 · ONNX Runtime Web";
        }
        setStatus({
          ...initialStatus,
          supported,
          phase: supported ? "failed" : "unsupported",
          runtime,
          error:
            cause instanceof Error
              ? cause.message
              : "Kivo could not read Voz status. Retry or reopen Kivo.",
        });
      });
    return () => {
      active = false;
    };
  }, [context.platform, onStatus]);

  async function install() {
    setBusy(true);
    setStatus((current) => ({ ...current, phase: "downloading", progress: null, error: null }));
    try {
      await nativeBridge.downloadVozModel();
      const next = await nativeBridge.getVozModelStatus();
      setStatus(next);
      onStatus?.(next);
    } catch (cause) {
      const error = cause instanceof Error ? cause.message : "Voz could not be installed.";
      setStatus((current) => ({ ...current, phase: "failed", error, progress: null }));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    await nativeBridge.deleteVozModel();
    const next = await nativeBridge.getVozModelStatus();
    setStatus(next);
    onStatus?.(next);
  }

  const isWorking = status.phase === "downloading" || status.phase === "preparing";
  const platformInfo = {
    windows: {
      size: "about 390 MB",
      note: "Uses WebGPU when available, with the SDK’s CPU fallback. The browser runtime can use about 1.2 GB of memory while loaded.",
    },
    linux: {
      size: "not available in the Linux test harness",
      note: "Voz runs only on supported Windows releases. The Linux harness keeps simulated speech for development and tests.",
    },
  }[context.platform];
  let modelAction: ReactNode;
  if (isWorking) {
    modelAction = (
      <div {...stylex.props(styles.progress)}>
        <ProgressBar
          hasValueLabel={status.phase === "downloading" && status.progress !== null}
          isIndeterminate={status.progress === null}
          label={
            status.phase === "downloading"
              ? "Downloading Voz and language models"
              : "Preparing Voz model"
          }
          value={(status.progress ?? 0) * 100}
        />
      </div>
    );
  } else if (status.downloaded) {
    modelAction = (
      <div {...stylex.props(styles.actions)}>
        <span {...stylex.props(styles.ready)}>Downloaded and prepared</span>
        <Button label="Remove model" onClick={() => void remove()} variant="ghost" />
      </div>
    );
  } else {
    modelAction = (
      <Button
        isDisabled={!status.supported || busy}
        isLoading={busy}
        label="Download and prepare Voz"
        onClick={() => void install()}
        variant="secondary"
      />
    );
  }

  return (
    <div {...stylex.props(styles.root)}>
      <div {...stylex.props(styles.description)}>
        <p>
          Voz is an optional on-device speech model. Kivo also downloads Ear as a separate model to
          check the language locally before transcription. Audio stays on this computer.
        </p>
        <p>
          Voz files: {platformInfo.size} · 25 supported languages · {status.runtime}
        </p>
        <p>{platformInfo.note}</p>
      </div>
      {status.phase === "unsupported" ? (
        <Banner status="warning" title="Voz is unavailable on this platform." />
      ) : null}
      {status.error ? <Banner status="error" title={status.error} /> : null}
      {modelAction}
    </div>
  );
}

const styles = stylex.create({
  root: { display: "flex", flexDirection: "column", gap: "var(--spacing-3)" },
  description: {
    display: "flex",
    flexDirection: "column",
    gap: "var(--spacing-1)",
    color: "var(--color-text-secondary)",
  },
  progress: { display: "flex", alignItems: "center", gap: "var(--spacing-3)" },
  actions: { display: "flex", alignItems: "center", gap: "var(--spacing-3)" },
  ready: { color: "var(--color-text-secondary)" },
});
