import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Icon, type IconName } from "../../components/Icon";
import { SegmentedControl } from "../../components/SegmentedControl";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";
import { StatusIndicator } from "../../components/StatusIndicator";
import { Switch } from "../../components/Switch";
import { nativeBridge, type UpdateResult } from "../../platform/native";
import {
  DEFAULT_WRITING_ACTIONS,
  NativeError,
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  type MicrophoneDevice,
  type SpeechLanguage,
} from "../../types";
import { writingAction } from "../writing-tools/actions";

type SettingsSection = "general" | "dictation" | "writing" | "ai" | "about";

interface SettingsWindowProps {
  context: AppContext;
  settings: AppSettings;
  loading: boolean;
  updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
}

const SECTIONS: Array<{ id: SettingsSection; label: string; icon: IconName }> = [
  { id: "general", label: "General", icon: "settings" },
  { id: "dictation", label: "Dictation", icon: "microphone" },
  { id: "writing", label: "Writing Tools", icon: "pencil" },
  { id: "ai", label: "AI", icon: "spark" },
  { id: "about", label: "About", icon: "audio" },
];

export function SettingsWindow({ context, settings, loading, updateSettings }: SettingsWindowProps) {
  const [section, setSection] = useState<SettingsSection>("general");
  const [microphones, setMicrophones] = useState<MicrophoneDevice[]>([]);
  const [languages, setLanguages] = useState<SpeechLanguage[]>([]);
  const [apiStatus, setApiStatus] = useState<ApiKeyStatus>({ configured: false, connection: "untested" });
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateResult | null>(null);

  useEffect(() => {
    let active = true;
    void Promise.all([nativeBridge.listMicrophones(), nativeBridge.listSpeechLanguages(), nativeBridge.getApiKeyStatus()])
      .then(([nextMicrophones, nextLanguages, nextApiStatus]) => {
        if (!active) return;
        setMicrophones(nextMicrophones);
        setLanguages(nextLanguages);
        setApiStatus(nextApiStatus);
      })
      .catch(() => active && setNotice("Some settings aren’t available right now."));
    return () => {
      active = false;
    };
  }, []);

  const save = useCallback(async (patch: Partial<AppSettings>) => {
    setNotice(null);
    try {
      await updateSettings(patch);
    } catch (error) {
      setNotice(error instanceof NativeError ? error.message : "The setting couldn’t be saved.");
    }
  }, [updateSettings]);

  const activeContent = useMemo(() => {
    switch (section) {
      case "general":
        return (
          <SettingsContent title="General" subtitle="Choose how Kivo behaves when you sign in and while it is idle.">
            <SettingsGroup>
              <SettingRow label="Launch at login" description="Start Kivo automatically after you sign in.">
                <Switch checked={settings.launchAtLogin} label="Launch at login" onChange={(value) => void save({ launchAtLogin: value })} />
              </SettingRow>
              <SettingRow label="Appearance">
                <SegmentedControl
                  ariaLabel="Appearance"
                  onChange={(theme) => void save({ theme })}
                  options={[{ label: "System", value: "system" }, { label: "Light", value: "light" }, { label: "Dark", value: "dark" }]}
                  value={settings.theme}
                />
              </SettingRow>
              <SettingRow label="Show Flow Bar while idle" description="Keep a quiet indicator visible between dictations.">
                <Switch checked={settings.showIdleFlowBar} label="Show Flow Bar while idle" onChange={(value) => void save({ showIdleFlowBar: value })} />
              </SettingRow>
              <SettingRow label="Start in background" description="Open without showing Settings.">
                <Switch checked={settings.startInBackground} label="Start in background" onChange={(value) => void save({ startInBackground: value })} />
              </SettingRow>
            </SettingsGroup>
          </SettingsContent>
        );
      case "dictation":
        return (
          <SettingsContent title="Dictation" subtitle="Hold your shortcut, speak, then release to insert text in the active app.">
            <SettingsGroup>
              <SettingRow label="Shortcut" description={context.platform === "macos" && settings.dictationShortcut === "Fn" ? "Fn is best-effort when macOS assigns the Globe key to another action." : undefined}>
                <ShortcutRecorder label="Dictation shortcut" onChange={(dictationShortcut) => save({ dictationShortcut })} platform={context.platform} value={settings.dictationShortcut} />
              </SettingRow>
              <SettingRow label="Microphone">
                <select aria-label="Microphone" onChange={(event) => void save({ microphoneId: event.target.value || null })} value={settings.microphoneId ?? ""}>
                  <option value="">System Default</option>
                  {microphones.filter((device) => device.id !== "default").map((device) => <option key={device.id} value={device.id}>{device.name}</option>)}
                </select>
              </SettingRow>
              <SettingRow label="Language" description="Automatic follows the current input language when supported.">
                <select aria-label="Dictation language" onChange={(event) => void save({ dictationLanguage: event.target.value })} value={settings.dictationLanguage}>
                  {languages.map((language) => <option key={language.code} value={language.code}>{language.name}{language.downloadable && !language.installed ? " — download required" : ""}</option>)}
                </select>
              </SettingRow>
            </SettingsGroup>
            <SettingsGroup>
              <SettingRow label="Improve dictated text with AI" description="Cleans punctuation and obvious filler words without changing your meaning.">
                <Switch checked={settings.improveDictationWithAi} label="Improve dictated text with AI" onChange={(value) => void save({ improveDictationWithAi: value })} />
              </SettingRow>
              <SettingRow label="Sound feedback" description="Play restrained start and finish sounds.">
                <Switch checked={settings.soundFeedback} label="Sound feedback" onChange={(value) => void save({ soundFeedback: value })} />
              </SettingRow>
            </SettingsGroup>
          </SettingsContent>
        );
      case "writing":
        return (
          <SettingsContent title="Writing Tools" subtitle="Choose the actions shown when you work with selected text.">
            <SettingsGroup>
              <SettingRow label="Shortcut">
                <ShortcutRecorder label="Writing Tools shortcut" onChange={(writingShortcut) => save({ writingShortcut })} platform={context.platform} value={settings.writingShortcut} />
              </SettingRow>
            </SettingsGroup>
            <SettingsGroup header="Actions">
              {DEFAULT_WRITING_ACTIONS.map((id) => {
                const action = writingAction(id);
                const checked = settings.enabledWritingActions.includes(id);
                return (
                  <SettingRow key={id} label={action.label} description={action.description}>
                    <Switch
                      checked={checked}
                      disabled={checked && settings.enabledWritingActions.length === 1}
                      label={`Show ${action.label}`}
                      onChange={(enabled) => {
                        const next = enabled
                          ? [...settings.enabledWritingActions, id]
                          : settings.enabledWritingActions.filter((actionId) => actionId !== id);
                        void save({ enabledWritingActions: next });
                      }}
                    />
                  </SettingRow>
                );
              })}
              <div className="settings-group__footer"><Button compact onClick={() => void save({ enabledWritingActions: [...DEFAULT_WRITING_ACTIONS] })}>Reset actions</Button></div>
            </SettingsGroup>
          </SettingsContent>
        );
      case "ai":
        return (
          <SettingsContent title="AI" subtitle="Kivo sends only the text needed for your request directly to Google Gemini.">
            <SettingsGroup>
              <SettingRow label="Google AI Studio API key" description={apiStatus.configured ? "A key is stored securely by the operating system." : "Required for writing actions and optional dictation cleanup."} stacked>
                <div className="api-key-editor">
                  <input
                    aria-label="Google AI Studio API key"
                    autoCapitalize="none"
                    autoComplete="off"
                    onChange={(event) => setApiKey(event.target.value)}
                    placeholder={apiStatus.configured ? "Enter a replacement key" : "Enter API key"}
                    spellCheck={false}
                    type="password"
                    value={apiKey}
                  />
                  <Button
                    compact
                    disabled={apiKey.trim().length < 8 || busy !== null}
                    onClick={() => {
                      setBusy("save-key");
                      setNotice(null);
                      void nativeBridge.saveApiKey(apiKey.trim())
                        .then((status) => {
                          setApiStatus(status);
                          setApiKey("");
                          setNotice("API key saved securely.");
                        })
                        .catch((error: unknown) => setNotice(error instanceof NativeError ? error.message : "The API key couldn’t be saved."))
                        .finally(() => setBusy(null));
                    }}
                    tone="primary"
                  >
                    {busy === "save-key" ? "Saving…" : "Save key"}
                  </Button>
                </div>
              </SettingRow>
              <SettingRow label="Connection" description={connectionDescription(apiStatus)}>
                <StatusIndicator label={connectionLabel(apiStatus)} state={apiStatus.connection} />
              </SettingRow>
              <div className="settings-group__footer settings-group__footer--split">
                <Button
                  compact
                  disabled={!apiStatus.configured || busy !== null}
                  onClick={() => {
                    setBusy("test-key");
                    setNotice(null);
                    setApiStatus((current) => ({ ...current, connection: "testing" }));
                    void nativeBridge.testApiKey()
                      .then(setApiStatus)
                      .catch((error: unknown) => setNotice(error instanceof NativeError ? error.message : "Couldn’t connect to Gemini."))
                      .finally(() => setBusy(null));
                  }}
                >
                  {busy === "test-key" ? "Testing…" : "Test connection"}
                </Button>
                {apiStatus.configured ? (
                  <Button
                    compact
                    onClick={() => {
                      setBusy("clear-key");
                      setNotice(null);
                      void nativeBridge.clearApiKey()
                        .then(setApiStatus)
                        .catch((error: unknown) => setNotice(error instanceof NativeError ? error.message : "The API key couldn’t be removed."))
                        .finally(() => setBusy(null));
                    }}
                    tone="danger"
                  >Remove key</Button>
                ) : null}
              </div>
            </SettingsGroup>
            <button className="text-link" onClick={() => void nativeBridge.openExternal("https://aistudio.google.com/app/apikey").catch(() => setNotice("Google AI Studio couldn’t be opened."))} type="button">Get an API key from Google AI Studio</button>
          </SettingsContent>
        );
      case "about":
        return (
          <SettingsContent title="About" subtitle="A quiet writing and dictation utility for your desktop.">
            <div className="about-lockup"><div className="about-lockup__mark"><Icon name="audio" size={27} /></div><div><h2>Kivo</h2><p>Version {context.version}</p></div></div>
            <SettingsGroup>
              <SettingRow label="Software updates" description={updateResult ? (updateResult.available ? `Version ${updateResult.availableVersion} is available.` : "Kivo is up to date.") : "Check manually for a newer version."}>
                <Button
                  compact
                  disabled={busy !== null}
                  onClick={() => {
                    setBusy("updates");
                    setNotice(null);
                    void nativeBridge.checkForUpdates()
                      .then(setUpdateResult)
                      .catch((error: unknown) => setNotice(error instanceof NativeError ? error.message : "Kivo couldn’t check for updates right now."))
                      .finally(() => setBusy(null));
                  }}
                >{busy === "updates" ? "Checking…" : "Check now"}</Button>
              </SettingRow>
            </SettingsGroup>
            <div className="about-links" aria-label="Project links">
              <span>Project website <small>Available at release</small></span>
              <span>Source repository <small>Available at release</small></span>
            </div>
            <p className="privacy-note">No accounts, analytics, or telemetry. Your text is processed only when you invoke Kivo.</p>
          </SettingsContent>
        );
    }
  }, [apiKey, apiStatus, busy, context.platform, context.version, languages, microphones, save, section, settings, updateResult]);

  return (
    <main className="settings-window" data-loading={loading} data-platform={context.platform}>
      <aside className="settings-sidebar" aria-label="Settings sections">
        <div className="settings-sidebar__brand"><span className="settings-sidebar__mark"><Icon name="audio" size={17} /></span><strong>Kivo</strong></div>
        <nav>
          {SECTIONS.map((item) => (
            <button aria-current={section === item.id ? "page" : undefined} key={item.id} onClick={() => setSection(item.id)} type="button">
              <Icon name={item.icon} size={16} /><span>{item.label}</span>
            </button>
          ))}
        </nav>
        <p className="settings-sidebar__status"><span />Running in the background</p>
      </aside>
      <div className="settings-main" tabIndex={-1}>
        {activeContent}
        {notice ? <div aria-live="polite" className="settings-notice">{notice}<button aria-label="Dismiss message" onClick={() => setNotice(null)} type="button"><Icon name="close" size={12} /></button></div> : null}
      </div>
    </main>
  );
}

function SettingsContent({ title, subtitle, children }: { title: string; subtitle: string; children: ReactNode }) {
  return <section className="settings-content"><header><h1>{title}</h1><p>{subtitle}</p></header>{children}</section>;
}

function SettingsGroup({ header, children }: { header?: string; children: ReactNode }) {
  return <section className="settings-group">{header ? <h2>{header}</h2> : null}<div className="settings-group__body">{children}</div></section>;
}

function SettingRow({ label, description, children, stacked = false }: { label: string; description?: string; children: ReactNode; stacked?: boolean }) {
  return <div className="setting-row" data-stacked={stacked}><div className="setting-row__label"><strong>{label}</strong>{description ? <span>{description}</span> : null}</div><div className="setting-row__control">{children}</div></div>;
}

function connectionLabel(status: ApiKeyStatus) {
  if (!status.configured) return "Not configured";
  const labels: Record<ApiKeyStatus["connection"], string> = {
    untested: "Not tested", testing: "Testing", connected: "Connected", invalid: "Key not accepted", "rate-limited": "Rate limited", offline: "Offline",
  };
  return labels[status.connection];
}

function connectionDescription(status: ApiKeyStatus) {
  if (!status.configured) return "Add a key to connect Kivo to Gemini.";
  if (status.connection === "connected") return "Gemini is ready for writing requests.";
  if (status.connection === "invalid") return "Check the key and save it again.";
  if (status.connection === "rate-limited") return "Gemini is temporarily rate limited. Try again shortly.";
  if (status.connection === "offline") return "Kivo couldn’t reach Gemini. Check your connection.";
  return "Test the saved key before using Writing Tools.";
}
