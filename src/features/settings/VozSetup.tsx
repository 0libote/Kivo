import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useState } from "react";
import { Switch } from "../../components/Switch";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { AppContext, AppSettings, VozModelProgress, VozModelStatus } from "../../types";

import { type SaveSettings, SettingRow } from "./settings-layout";

interface VozSetupProps {
  readonly settings: AppSettings;
  readonly save: SaveSettings;
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

export function VozSetup({ context, onStatus, settings, save }: VozSetupProps) {
  const [status, setStatus] = useState(() => {
    const supported = !nativeBridge.isNative || context.platform !== "linux";
    return {
      ...initialStatus,
      supported,
      phase: supported ? ("notDownloaded" as const) : initialStatus.phase,
    };
  });
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    onStatus?.(status);
  }, [status, onStatus]);

  useNativeEvent<VozModelProgress>("voz-model-status", (update) => {
    setStatus((current) => {
      const next = {
        ...current,
        phase: update.phase,
        progress: update.progress,
        error: update.error,
        downloaded: update.phase === "ready" || current.downloaded,
      };
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
  }, [context.platform]);

  async function install() {
    setBusy(true);
    setStatus((current) => ({ ...current, phase: "downloading", progress: null, error: null }));
    try {
      await nativeBridge.downloadVozModel();
      const next = await nativeBridge.getVozModelStatus();
      setStatus(next);
    } catch (cause) {
      const error = cause instanceof Error ? cause.message : "Voz could not be installed.";
      setStatus((current) => ({ ...current, phase: "failed", error, progress: null }));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (busy) return;
    setBusy(true);
    try {
      await nativeBridge.deleteVozModel();
      const next = await nativeBridge.getVozModelStatus();
      setStatus(next);
    } catch (cause) {
      setStatus((current) => ({
        ...current,
        error: cause instanceof Error ? cause.message : "Voz could not be removed.",
      }));
    } finally {
      setBusy(false);
    }
  }

  async function openCredits(url = "https://desertant.com") {
    try {
      await nativeBridge.openExternal(url);
    } catch (cause) {
      setStatus((current) => ({
        ...current,
        error: cause instanceof Error ? cause.message : "Could not open model credits.",
      }));
    }
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
        <Button
          isDisabled={busy}
          isLoading={busy}
          label="Remove model"
          onClick={() => void remove()}
          variant="ghost"
        />
      </div>
    );
  } else {
    modelAction = (
      <Button
        isDisabled={!status.supported || busy || !settings.desertAntUsageAccepted}
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
      <Banner
        status="info"
        title="Optional model with required usage reporting"
        description="Voz and Ear process audio on this computer. Their SDKs send device-usage telemetry to Desert Ant Labs to count monthly active devices. Desert Ant says this uses an SDK-generated identifier and excludes audio, transcripts and other user content. Desert Ant controls this telemetry; it cannot be disabled while using these models. Download and reporting servers also receive your IP address."
      />
      <Button
        label="Read Desert Ant license and data collection terms"
        onClick={() => void openCredits("https://license.desertant.com/1.0")}
        variant="ghost"
      />
      <SettingRow
        label="Allow Desert Ant usage reporting"
        description="Required to download or use Voz/Ear. Turn this off to stop using these SDKs; System and native On-device models remain available."
      >
        <Switch
          label="Allow Desert Ant usage reporting"
          checked={settings.desertAntUsageAccepted}
          disabled={busy}
          onChange={(accepted) =>
            void save({
              desertAntUsageAccepted: accepted,
              ...(!accepted && settings.speechEngine === "voz"
                ? { speechEngine: "system" as const }
                : {}),
            })
          }
        />
      </SettingRow>
      {modelAction}
      <Button
        label="Voz and Ear by Desert Ant Labs · model credits"
        onClick={() => void openCredits()}
        variant="ghost"
      />
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
