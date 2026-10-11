import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { nativeBridge } from "../../platform/native";
import { type AppSettings, type LocalAiServerInfo, NativeError } from "../../types";

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
  const [error, setError] = useState<string | null>(null);

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
    try {
      await nativeBridge.openExternal("https://ollama.com/download/windows");
    } catch (error_) {
      setError(
        error_ instanceof NativeError ? error_.message : "The download page couldn’t be opened.",
      );
    } finally {
      setInstalling(false);
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
          label={installing ? "Opening…" : "Download Ollama"}
          onClick={() => void install()}
          size="sm"
        />
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
    gap: "var(--spacing-2)",
  },
  list: {
    display: "grid",
    gap: "var(--spacing-2)",
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "var(--spacing-3)",
    padding: "var(--spacing-2) var(--spacing-3)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  rowActive: {
    borderColor: "var(--color-accent)",
  },
  labels: {
    display: "grid",
    gap: "var(--spacing-1)",
  },
  name: {
    fontSize: "var(--font-size-sm)",
    fontWeight: "var(--font-weight-semibold)",
  },
  meta: {
    color: "var(--color-text-secondary)",
    fontSize: "var(--font-size-xs)",
  },
  empty: {
    display: "grid",
    gap: "var(--spacing-2)",
    justifyItems: "start",
  },
  footer: {
    display: "flex",
  },
  note: {
    margin: "var(--spacing-1) 0 var(--spacing-3)",
    color: "var(--color-text-secondary)",
    fontSize: "var(--font-size-sm)",
    lineHeight: "normal",
  },
});
