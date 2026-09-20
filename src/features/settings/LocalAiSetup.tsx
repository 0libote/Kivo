import { useCallback, useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import {
  NativeError,
  type AppSettings,
  type LocalAiInstallProgress,
  type LocalAiServerInfo,
} from "../../types";

interface LocalAiSetupProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
  readonly disabled: boolean;
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
    } catch (caught) {
      setError(caught instanceof NativeError ? caught.message : "The server couldn’t be selected.");
    }
  }

  async function install() {
    setInstalling(true);
    setError(null);
    setProgress(null);
    try {
      await nativeBridge.installLocalAiRuntime();
      setError("Finish the Ollama install, then Refresh to detect it.");
    } catch (caught) {
      setError(caught instanceof NativeError ? caught.message : "The installer couldn’t be downloaded.");
    } finally {
      setInstalling(false);
      setProgress(null);
    }
  }

  return (
    <div className="local-ai">
      {scanning ? (
        <p className="settings-note">Checking for local servers…</p>
      ) : running.length > 0 ? (
        <div className="local-ai__list">
          {running.map((server) => {
            const active = settings.aiCustomBaseUrl === server.baseUrl;
            return (
              <div className="local-ai__row" data-active={active} key={server.id}>
                <span className="local-ai__labels">
                  <strong>{server.name}</strong>
                  <span>{server.models.length > 0 ? `${server.models.length} model${server.models.length === 1 ? "" : "s"} ready` : "No models pulled yet"}</span>
                </span>
                <Button compact disabled={disabled || active} onClick={() => void useServer(server)} tone={active ? undefined : "primary"}>
                  {active ? "Selected" : "Use"}
                </Button>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="local-ai__empty">
          <p className="settings-note">No local server detected. Install Ollama to run models on this computer for free.</p>
          <Button compact disabled={disabled || installing} onClick={() => void install()}>
            {installing ? "Downloading…" : "Install Ollama"}
          </Button>
          {progress && progress.total > 0 ? (
            <progress aria-label="Downloading Ollama" max={progress.total} value={progress.downloaded} />
          ) : null}
        </div>
      )}
      <div className="local-ai__footer">
        <Button compact disabled={disabled || scanning} onClick={scan}>Refresh</Button>
      </div>
      {error ? <p className="settings-note" role="alert">{error}</p> : null}
    </div>
  );
}
