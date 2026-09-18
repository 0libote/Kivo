import { useCallback, useEffect, useState } from "react";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type {
  AppContext,
  AppSettings,
  DictationSnapshot,
  PermissionStatus,
} from "../../types";

/**
 * Frontend-only dev bench (never a Tauri window, never IPC): one screen
 * that exercises every bit through the active bridge — MockBridge in
 * `bun dev`, the real Rust core under `bun tauri dev` on Linux.
 *
 * Read-only probes run on mount (permissions, mics, languages, models,
 * key status, recovery). Write paths are explicit buttons so nothing here
 * spends AI quota or overwrites a key by surprise.
 */
export function GalleryWindow({
  context,
  settings,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
}) {
  const [permissions, setPermissions] = useState<PermissionStatus[]>([]);
  const [mics, setMics] = useState<string>("—");
  const [languages, setLanguages] = useState<string>("—");
  const [models, setModels] = useState<string>("—");
  const [keyStatus, setKeyStatus] = useState<string>("—");
  const [recovery, setRecovery] = useState<string | null>(null);
  const [dictation, setDictation] = useState<DictationSnapshot | null>(null);
  const [writing, setWriting] = useState<string>("Not run yet.");
  const [roundTrip, setRoundTrip] = useState<string>("Not run yet.");
  const [error, setError] = useState<string | null>(null);

  useNativeEvent<DictationSnapshot>("dictation-state", setDictation);

  const refresh = useCallback(() => {
    setError(null);
    void (async () => {
      try {
        const [perms, micList, langList, modelList, key, rec] = await Promise.all([
          nativeBridge.getPermissions(),
          nativeBridge.listMicrophones(),
          nativeBridge.listSpeechLanguages(),
          nativeBridge.listAiModels(),
          nativeBridge.getApiKeyStatus(),
          nativeBridge.getDictationRecovery(),
        ]);
        setPermissions(perms);
        setMics(micList.map((mic) => mic.name).join(", "));
        setLanguages(langList.map((lang) => `${lang.code}${lang.installed ? "" : " (missing)"}`).join(", "));
        setModels(modelList.map((model) => model.id).join(", "));
        setKeyStatus(
          `configured=${key.configured ? "yes" : "no"} connection=${key.connection}`,
        );
        setRecovery(rec);
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : "The probe failed.");
      }
    })();
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function runWritingProbe() {
    setWriting("Running proofread on the sample text…");
    try {
      const response = await nativeBridge.runWritingAction({
        action: "proofread",
        text: "Hello, how are you?",
      });
      setWriting(
        response.kind === "replaced"
          ? "Replaced the text in place."
          : (response.text ?? "(empty result)"),
      );
    } catch (caught) {
      setWriting(caught instanceof Error ? `Failed: ${caught.message}` : "Failed.");
    }
  }

  async function runSettingsRoundTrip() {
    setRoundTrip("Writing soundFeedback=false then restoring…");
    try {
      const original = settings.soundFeedback;
      await nativeBridge.updateSettings({ soundFeedback: false });
      const restored = await nativeBridge.updateSettings({ soundFeedback: original });
      setRoundTrip(
        restored.soundFeedback === original
          ? "Round trip ok: write + restore both persisted."
          : "Round trip mismatch: restore did not persist.",
      );
      refresh();
    } catch (caught) {
      setRoundTrip(caught instanceof Error ? `Failed: ${caught.message}` : "Failed.");
      refresh();
    }
  }

  return (
    <main className="settings-window">
      <aside className="settings-sidebar">
        <div className="settings-sidebar__brand">
          <span aria-hidden="true" className="settings-sidebar__mark">K</span>
          <span>Kivo bench</span>
        </div>
        <p className="settings-sidebar__version">
          {context.platform} · {nativeBridge.isNative ? "native core" : "mock harness"} · {context.version || "dev"}
        </p>
        <p className="settings-sidebar__status">
          {context.paused ? "Paused" : "Active"}
        </p>
      </aside>
      <section className="settings-main">
        <div className="settings-content">
          <h1>Test bench</h1>
          <p className="settings-note">
            One screen over the active bridge. Green here means the shared
            AppCore path works; only the thin per-OS adapter differs on
            macOS and Windows.
          </p>
          {error ? (
            <p className="settings-notice" role="alert">{error}</p>
          ) : null}

          <div className="settings-group">
            <h2>Contract</h2>
            <div className="settings-group__body">
              <div className="setting-row">
                <span className="setting-row__label">Dictation shortcut</span>
                <span className="setting-row__control">{settings.dictationShortcut}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Writing shortcut</span>
                <span className="setting-row__control">{settings.writingShortcut}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">AI provider / queue</span>
                <span className="setting-row__control">
                  {settings.aiProvider} · {settings.aiModels.join(", ")}
                </span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Permissions</span>
                <span className="setting-row__control">
                  {permissions.length === 0
                    ? "—"
                    : permissions.map((p) => `${p.kind}=${p.state}${p.required ? "*" : ""}`).join(" · ")}
                </span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Microphones</span>
                <span className="setting-row__control">{mics}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Speech languages</span>
                <span className="setting-row__control">{languages}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">AI models</span>
                <span className="setting-row__control">{models}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">API key</span>
                <span className="setting-row__control">{keyStatus}</span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Recovery text</span>
                <span className="setting-row__control">
                  {recovery === null ? "(none)" : recovery}
                </span>
              </div>
              <div className="setting-row">
                <span className="setting-row__label">Dictation event</span>
                <span className="setting-row__control">
                  {dictation === null ? "(no event yet)" : dictation.status}
                </span>
              </div>
            </div>
            <div className="settings-group__footer">
              <button className="button button--compact" onClick={refresh} type="button">
                Re-run probes
              </button>
            </div>
          </div>

          <div className="settings-group">
            <h2>Dictation smoke</h2>
            <p className="settings-note">
              Drives the real start/stop/cancel path. On Linux the simulated
              engine answers; in the mock harness the canned events answer.
            </p>
            <div className="settings-group__footer settings-group__footer--split">
              <button
                className="button button--compact"
                onClick={() => void nativeBridge.startDictation().catch((caught: unknown) => setError(caught instanceof Error ? caught.message : "start failed"))}
                type="button"
              >
                Start
              </button>
              <button
                className="button button--compact"
                onClick={() => void nativeBridge.stopDictation().catch((caught: unknown) => setError(caught instanceof Error ? caught.message : "stop failed"))}
                type="button"
              >
                Stop
              </button>
              <button
                className="button button--compact"
                onClick={() => void nativeBridge.cancelDictation().catch((caught: unknown) => setError(caught instanceof Error ? caught.message : "cancel failed"))}
                type="button"
              >
                Cancel
              </button>
            </div>
          </div>

          <div className="settings-group">
            <h2>Writing smoke</h2>
            <p className="settings-note">
              Runs proofread on the sample text. Uses AI quota when a key is
              configured against the live core.
            </p>
            <div className="settings-group__body">
              <div className="setting-row">
                <span className="setting-row__label">Result</span>
                <span className="setting-row__control">{writing}</span>
              </div>
            </div>
            <div className="settings-group__footer">
              <button className="button button--compact" onClick={() => void runWritingProbe()} type="button">
                Run proofread
              </button>
            </div>
          </div>

          <div className="settings-group">
            <h2>Settings round trip</h2>
            <div className="settings-group__body">
              <div className="setting-row">
                <span className="setting-row__label">Result</span>
                <span className="setting-row__control">{roundTrip}</span>
              </div>
            </div>
            <div className="settings-group__footer">
              <button className="button button--compact" onClick={() => void runSettingsRoundTrip()} type="button">
                Write + restore
              </button>
            </div>
          </div>

          <div className="settings-group">
            <h2>Surfaces</h2>
            <div className="settings-group__footer settings-group__footer--split">
              <a className="button button--compact" href="?surface=flow-bar&harness=1">Flow Bar</a>
              <a className="button button--compact" href="?surface=writing-tools&harness=1">Writing Tools</a>
              <a className="button button--compact" href="?surface=settings&harness=1">Settings</a>
              <a className="button button--compact" href="?surface=onboarding&harness=1">Onboarding</a>
            </div>
          </div>
        </div>
      </section>
    </main>
  );
}
