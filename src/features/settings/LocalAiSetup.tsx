import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import {
  type AppSettings,
  type LocalAiInstallProgress,
  type LocalAiServerInfo,
  NativeError,
} from "../../types";

interface LocalAiSetupProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
  readonly disabled: boolean;
}

function modelsLabel(count: number): string {
  if (count === 0) return "No models pulled yet";
  return `${count} model${count === 1 ? "" : "s"} ready`;
}

/**
 * Detects OpenAI-compatible servers running on this machine and offers to
 * install Ollama when none is found. All of it is optional: the user can also
 * type any server URL by hand.
 */
export function LocalAiSetup({ settings, save, disabled }: LocalAiSetupProps) {
  const [servers, setServers] = useState<LocalAiServerInfo[]>([]);
  const [scanning, setScanning] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LocalAiInstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useNativeEvent<LocalAiInstallProgress>("local-ai-install-progress", setProgress);

  const scan = useCallback(() => {
    setScanning(true);
    setError(null);
    void nativeBridge
      .detectLocalAiServers()
      .then(setServers)
      .catch(() => setError("Couldn’t check for local servers."))
      .finally(() => setScanning(false));
  }, []);

  useEffect(scan, [scan]);

  const running = servers.filter((server) => server.running);

  async function useServer(server: LocalAiServerInfo) {
    setError(null);
    try {
      await save({ aiCustomBaseUrl: server.baseUrl });
    } catch (error_) {
      setError(error_ instanceof NativeError ? error_.message : "The server couldn’t be selected.");
    }
  }

  async function install() {
    setInstalling(true);
    setError(null);
    setProgress(null);
    try {
      await nativeBridge.installLocalAiRuntime();
      setError("Finish the Ollama install, then Refresh to detect it.");
    } catch (error_) {
      setError(
        error_ instanceof NativeError ? error_.message : "The installer couldn’t be downloaded.",
      );
    } finally {
      setInstalling(false);
      setProgress(null);
    }
  }

  let body: ReactNode;
  if (scanning) {
    body = <p {...stylex.props(styles.note)}>Checking for local servers…</p>;
  } else if (running.length > 0) {
    body = (
      <div {...stylex.props(styles.list)}>
        {running.map((server) => {
          const active = settings.aiCustomBaseUrl === server.baseUrl;
          return (
            <div key={server.id} {...stylex.props(styles.row, active && styles.rowActive)}>
              <span {...stylex.props(styles.labels)}>
                <strong {...stylex.props(styles.name)}>{server.name}</strong>
                <span {...stylex.props(styles.meta)}>{modelsLabel(server.models.length)}</span>
              </span>
              <Button
                isDisabled={disabled || active}
                label={active ? "Selected" : "Use"}
                onClick={() => void useServer(server)}
                size="sm"
                variant={active ? "secondary" : "primary"}
              />
            </div>
          );
        })}
      </div>
    );
  } else {
    body = (
      <div {...stylex.props(styles.empty)}>
        <p {...stylex.props(styles.note)}>
          No local server detected. Install Ollama to run models on this computer for free.
        </p>
        <Button
          isDisabled={disabled || installing}
          label={installing ? "Downloading…" : "Install Ollama"}
          onClick={() => void install()}
          size="sm"
        />
        {progress && progress.total > 0 ? (
          <ProgressBar
            isLabelHidden
            label="Downloading Ollama"
            max={progress.total}
            value={progress.downloaded}
            xstyle={styles.download}
          />
        ) : null}
      </div>
    );
  }

  return (
    <div {...stylex.props(styles.root)}>
      {body}
      <div {...stylex.props(styles.footer)}>
        <Button isDisabled={disabled || scanning} label="Refresh" onClick={scan} size="sm" />
      </div>
      {error ? <Banner status="error" title={error} /> : null}
    </div>
  );
}

const styles = stylex.create({
  root: {
    display: "grid",
    gap: "8px",
  },
  list: {
    display: "grid",
    gap: "8px",
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "12px",
    padding: "9px 12px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  rowActive: {
    borderColor: "var(--color-accent)",
  },
  labels: {
    display: "grid",
    gap: "2px",
  },
  name: {
    fontSize: "13px",
    fontWeight: 620,
  },
  meta: {
    color: "var(--color-text-secondary)",
    fontSize: "11.5px",
  },
  empty: {
    display: "grid",
    gap: "8px",
    justifyItems: "start",
  },
  download: {
    width: "220px",
  },
  footer: {
    display: "flex",
  },
  note: {
    margin: "2px 0 14px",
    color: "var(--color-text-secondary)",
    fontSize: "12px",
    lineHeight: 1.4,
  },
});
