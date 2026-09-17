import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { AiModelSelect } from "./AiModelSelect";
import { HomeSection } from "./HomeSection";
import { useNativeEvent } from "../../hooks/useNativeEvent";
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
  type PermissionKind,
  type PermissionStatus,
  type SpeechLanguage,
} from "../../types";
import { writingAction } from "../writing-tools/actions";

type SettingsSection = "home" | "general" | "dictation" | "writing" | "ai" | "permissions" | "about";

interface SettingsWindowProps {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly loading: boolean;
  readonly updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
}

const SECTIONS: Array<{ id: SettingsSection; label: string; icon: IconName }> = [
  { id: "home", label: "Home", icon: "home" },
  { id: "general", label: "General", icon: "settings" },
  { id: "dictation", label: "Dictation", icon: "microphone" },
  { id: "writing", label: "Writing Tools", icon: "pencil" },
  { id: "ai", label: "AI", icon: "connection" },
  { id: "permissions", label: "Permissions", icon: "check" },
  { id: "about", label: "About", icon: "info" },
];

type SaveSettings = (patch: Partial<AppSettings>) => Promise<void>;

export function SettingsWindow({ context, settings, loading, updateSettings }: SettingsWindowProps) {
  const [section, setSection] = useState<SettingsSection>("home");
  const [microphones, setMicrophones] = useState<MicrophoneDevice[]>([]);
  const [languages, setLanguages] = useState<SpeechLanguage[]>([]);
  const [apiStatus, setApiStatus] = useState<ApiKeyStatus>({ configured: false, connection: "untested" });
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateResult | null>(null);

  useNativeEvent("show-about", () => setSection("about"));

  useEffect(() => {
    let active = true;
    // allSettled never rejects; each list degrades independently below.
    void Promise.allSettled([nativeBridge.listMicrophones(), nativeBridge.listSpeechLanguages(), nativeBridge.getApiKeyStatus()])
      .then(([nextMicrophones, nextLanguages, nextApiStatus]) => {
        if (!active) return;
        if (nextMicrophones.status === "fulfilled") setMicrophones(nextMicrophones.value);
        if (nextLanguages.status === "fulfilled") setLanguages(nextLanguages.value);
        if (nextApiStatus.status === "fulfilled") setApiStatus(nextApiStatus.value);
        if ([nextMicrophones, nextLanguages, nextApiStatus].some(result => result.status === "rejected")) setNotice("Some settings could not be loaded. Reopen Settings to try again.");
      });
    return () => {
      active = false;
    };
  }, []);

  const save = useCallback<SaveSettings>(
    async (patch) => {
      setNotice(null);
      try {
        await updateSettings(patch);
      } catch (error) {
        setNotice(error instanceof NativeError ? error.message : "The setting couldn’t be saved.");
      }
    },
    [updateSettings],
  );

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
        <p className="settings-sidebar__status">Kivo <span className="settings-sidebar__version">{context.version}</span></p>
      </aside>
      <SettingsMain key={section}>
        {section === "home" ? <HomeSection context={context} settings={settings} onWriting={() => setSection("writing")} onDictation={() => setSection("dictation")} /> : <SectionContent
          apiKey={apiKey}
          apiStatus={apiStatus}
          busy={busy}
          context={context}
          languages={languages}
          microphones={microphones}
          section={section}
          setApiKey={setApiKey}
          setApiStatus={setApiStatus}
          setBusy={setBusy}
          setNotice={setNotice}
          setUpdateResult={setUpdateResult}
          settings={settings}
          save={save}
          updateResult={updateResult}
        />}
        {notice ? <div aria-live="polite" className="settings-notice">{notice}<button aria-label="Dismiss message" onClick={() => setNotice(null)} type="button"><Icon name="close" size={12} /></button></div> : null}
      </SettingsMain>
    </main>
  );
}

function SettingsMain({ children }: { readonly children: ReactNode }) {
  const mainRef = useRef<HTMLDivElement>(null);
  // Move focus to the new section on navigation only (mount), so keyboard and
  // screen-reader users land on the fresh heading. Runs once per section
  // because the parent remounts this component via key={section}.
  useEffect(() => {
    mainRef.current?.focus({ preventScroll: true });
  }, []);
  return <div className="settings-main" ref={mainRef} tabIndex={-1}>{children}</div>;
}

interface SectionContentProps {
  readonly apiKey: string;
  readonly apiStatus: ApiKeyStatus;
  readonly busy: string | null;
  readonly context: AppContext;
  readonly languages: SpeechLanguage[];
  readonly microphones: MicrophoneDevice[];
  readonly section: SettingsSection;
  readonly setApiKey: (value: string) => void;
  readonly setApiStatus: (value: ApiKeyStatus | ((current: ApiKeyStatus) => ApiKeyStatus)) => void;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly setUpdateResult: (value: UpdateResult) => void;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly updateResult: UpdateResult | null;
}

function SectionContent(props: SectionContentProps) {
  switch (props.section) {
    case "home": return null;
    case "permissions":
      return (
        <PermissionsSection
          context={props.context}
          settings={props.settings}
          setNotice={props.setNotice}
        />
      );
    case "general":
      return <GeneralSection settings={props.settings} save={props.save} />;
    case "dictation":
      return (
        <DictationSection
          context={props.context}
          languages={props.languages}
          microphones={props.microphones}
          settings={props.settings}
          save={props.save}
        />
      );
    case "writing":
      return <WritingSection context={props.context} settings={props.settings} save={props.save} />;
    case "ai":
      return (
        <AiSection
          apiKey={props.apiKey}
          apiStatus={props.apiStatus}
          busy={props.busy}
          setApiKey={props.setApiKey}
          setApiStatus={props.setApiStatus}
          setBusy={props.setBusy}
          setNotice={props.setNotice}
          settings={props.settings}
          save={props.save}
        />
      );
    case "about":
      return (
        <AboutSection
          busy={props.busy}
          context={props.context}
          setBusy={props.setBusy}
          setNotice={props.setNotice}
          setUpdateResult={props.setUpdateResult}
          updateResult={props.updateResult}
        />
      );
  }
}

function PermissionsSection({
  context,
  settings,
  setNotice,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly setNotice: (value: string | null) => void;
}) {
  const [permissions, setPermissions] = useState<PermissionStatus[]>([]);
  const [busy, setBusy] = useState<PermissionKind | null>(null);
  const [loaded, setLoaded] = useState(false);
  useNativeEvent<PermissionStatus[]>("permission-status-changed", setPermissions);

  const refresh = useCallback(() => {
    void nativeBridge
      .getPermissions()
      .then((next) => {
        setPermissions(next);
      })
      .catch(() => setNotice("Permission status isn’t available right now."))
      .finally(() => setLoaded(true));
  }, [setNotice]);

  useEffect(refresh, [refresh]);

  async function request(kind: PermissionKind) {
    setBusy(kind);
    setNotice(null);
    try {
      // Windows has no in-app prompt: open the Settings page, then re-read
      // the (possibly changed) state instead of firing a no-op request.
      // macOS shows the native prompt directly.
      if (context.platform === "windows") {
        await nativeBridge.openPermissionSettings(kind);
        setPermissions(await nativeBridge.getPermissions());
      } else {
        setPermissions(await nativeBridge.requestPermission(kind));
      }
    } catch (error) {
      setNotice(error instanceof NativeError ? error.message : "Permission wasn’t granted. Try again.");
      refresh();
    } finally {
      setBusy(null);
    }
  }

  async function openSettings(kind: PermissionKind) {
    setBusy(kind);
    setNotice(null);
    try {
      await nativeBridge.openPermissionSettings(kind);
      setPermissions(await nativeBridge.getPermissions());
    } catch {
      setNotice("The system settings page couldn’t be opened.");
    } finally {
      setBusy(null);
    }
  }

  const subtitle =
    context.platform === "macos"
      ? "Allow access so Kivo can work with selected text and dictate. If a permission was denied, open Settings to allow it."
      : "Microphone access is managed in Windows Settings. Text access needs no extra prompt on Windows.";
  const order: PermissionKind[] = ["accessibility", "input-monitoring", "microphone", "speech-recognition"];
  const byKind = new Map(permissions.map((permission) => [permission.kind, permission]));

  return (
    <SettingsContent title="Permissions" subtitle={subtitle}>
      <SettingsGroup>
        {order.map((kind) => {
          const status = byKind.get(kind);
          const state = status?.state ?? "not-determined";
          if (state === "unavailable") {
            return (
              <SettingRow
                key={kind}
                label={permissionLabel(kind, settings.dictationShortcut)}
                description={status?.explanation ?? "Not required on this system."}
              >
                <span className="permission-row__granted">Not required</span>
              </SettingRow>
            );
          }
          const granted = state === "granted";
          const denied = state === "denied";
          return (
            <SettingRow
              key={kind}
              label={permissionLabel(kind, settings.dictationShortcut)}
              description={status?.explanation ?? permissionBlurb(kind, context.platform)}
            >
              <span className="permission-row__control">
                <StatusIndicator label={permissionLabel(kind, settings.dictationShortcut)} state={state} />
                {granted ? (
                  <span className="permission-row__granted"><Icon name="check" size={15} />Allowed</span>
                ) : (
                  <Button
                    compact
                    disabled={busy !== null || !loaded}
                    onClick={() => void (denied ? openSettings(kind) : request(kind))}
                  >
                    {permissionActionLabel(busy === kind, denied)}
                  </Button>
                )}
              </span>
            </SettingRow>
          );
        })}
      </SettingsGroup>
      <div className="settings-group__footer settings-group__footer--split">
        <Button compact disabled={busy !== null} onClick={refresh}>Refresh status</Button>
      </div>
    </SettingsContent>
  );
}

function permissionActionLabel(waiting: boolean, denied: boolean): string {
  if (waiting) return "Waiting…";
  if (denied) return "Open Settings";
  return "Allow";
}

function permissionLabel(kind: PermissionKind, dictationShortcut: string): string {
  switch (kind) {
    case "accessibility": return "Accessibility";
    case "input-monitoring": return dictationShortcut === "Fn" ? "Fn shortcut monitoring" : "Shortcut monitoring";
    case "microphone": return "Microphone";
    case "speech-recognition": return "Speech Recognition";
  }
}

function permissionBlurb(kind: PermissionKind, platform: AppContext["platform"]): string {
  switch (kind) {
    case "accessibility":
      return platform === "macos"
        ? "Read only the text you select and insert text where your cursor is."
        : "Work with the selected text and cursor in your active app.";
    case "input-monitoring": return "Detect the dictation hold shortcut.";
    case "microphone": return "Listen only while dictation is active.";
    case "speech-recognition": return "Transcribe speech using the operating system.";
  }
}

function GeneralSection({ settings, save }: { readonly settings: AppSettings; readonly save: SaveSettings }) {
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
        <SettingRow label="Start in background" description="Keep the main window closed when Kivo starts.">
          <Switch checked={settings.startInBackground} label="Start in background" onChange={(value) => void save({ startInBackground: value })} />
        </SettingRow>
      </SettingsGroup>
    </SettingsContent>
  );
}

function DictationSection({
  context,
  languages,
  microphones,
  settings,
  save,
}: {
  readonly context: AppContext;
  readonly languages: SpeechLanguage[];
  readonly microphones: MicrophoneDevice[];
  readonly settings: AppSettings;
  readonly save: SaveSettings;
}) {
  const shortcutNote =
    context.platform === "macos" && settings.dictationShortcut === "Fn"
      ? "Fn is best-effort when macOS assigns the Globe key to another action. Holding Fn suppresses its system Globe action while Kivo runs."
      : undefined;
  return (
    <SettingsContent title="Dictation" subtitle="Hold your shortcut, speak, then release — or tap to start and tap again to stop.">
      <SettingsGroup>
        <SettingRow label="Shortcut" description={shortcutNote}>
          <ShortcutRecorder label="Dictation shortcut" onChange={(dictationShortcut) => save({ dictationShortcut })} platform={context.platform} value={settings.dictationShortcut} />
        </SettingRow>
        <SettingRow label="Tap to dictate" description="A quick press starts listening; press again to finish.">
          <Switch checked={settings.dictationTapEnabled} label="Tap to dictate" onChange={(value) => void save({ dictationTapEnabled: value })} />
        </SettingRow>
        <SettingRow label="Hold to dictate" description="Keep the shortcut held while speaking; release to finish.">
          <Switch checked={settings.dictationHoldEnabled} label="Hold to dictate" onChange={(value) => void save({ dictationHoldEnabled: value })} />
        </SettingRow>
        <SettingRow label="Hold threshold" description="How long (ms) a press must last to count as a hold instead of a tap.">
          <NumberPreference label="Hold threshold" min={50} max={5000} value={settings.dictationHoldThresholdMs} onChange={(value) => save({ dictationHoldThresholdMs: value })} />
        </SettingRow>
        <SettingRow label="Microphone">
          {microphones.length === 0 ? (
            <span className="setting-empty">No microphones found. Check the system sound settings.</span>
          ) : (
            <select aria-label="Microphone" onChange={(event) => void save({ microphoneId: event.target.value || null })} value={microphones.some((device) => device.id === (settings.microphoneId ?? "")) || settings.microphoneId === null ? (settings.microphoneId ?? "") : ""}>
              <option value="">System Default</option>
              {microphones.filter((device) => device.id !== "default").map((device) => <option key={device.id} value={device.id}>{device.name}</option>)}
            </select>
          )}
        </SettingRow>
        <SettingRow label="Language" description={context.platform === "windows" ? "Automatic uses the system speech language. Only installed desktop speech languages can start dictation — install one in Windows Settings → Time & language → Speech." : "Automatic follows the current input language when supported."}>
          {languages.length === 0 ? (
            <span className="setting-empty">No languages found. Reopen Settings to try again.</span>
          ) : (
            <select aria-label="Dictation language" onChange={(event) => void save({ dictationLanguage: event.target.value })} value={languages.some((language) => language.code === settings.dictationLanguage) ? settings.dictationLanguage : languages[0].code}>
              {languages.map((language) => <option key={language.code} value={language.code}>{languageName(language)}</option>)}
            </select>
          )}
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
}

function languageName(language: SpeechLanguage): string {
  if (language.downloadable && !language.installed) return `${language.name} — download required`;
  return language.name;
}

function NumberPreference({ label, value, min, max, onChange }: {
  readonly label: string;
  readonly value: number;
  readonly min: number;
  readonly max: number;
  readonly onChange: (value: number) => Promise<void>;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return <input aria-label={label} type="number" min={min} max={max} value={draft ?? value}
    onChange={event => setDraft(event.target.value)}
    onBlur={event => {
      const next = event.currentTarget.valueAsNumber;
      setDraft(null);
      if (Number.isFinite(next) && next !== value) void onChange(Math.min(max, Math.max(min, next)));
    }}
    onKeyDown={event => {
      if (event.key === "Escape") event.currentTarget.value = String(value);
      if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
    }} />;
}

function WritingSection({
  context,
  settings,
  save,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
}) {
  return (
    <SettingsContent title="Writing Tools" subtitle="Choose the actions shown when you work with selected text.">
      <SettingsGroup>
        <SettingRow label="Shortcut">
          <ShortcutRecorder label="Writing Tools shortcut" onChange={(writingShortcut) => save({ writingShortcut })} platform={context.platform} value={settings.writingShortcut} />
        </SettingRow>
        <SettingRow label="Open beside" description="Choose where Writing Tools appears.">
          <SegmentedControl
            ariaLabel="Popup placement"
            onChange={(writingPopupAnchor) => void save({ writingPopupAnchor })}
            options={[{ label: "Cursor", value: "cursor" }, { label: "Selection", value: "selection" }, { label: "Fixed", value: "fixed" }]}
            value={settings.writingPopupAnchor}
          />
        </SettingRow>
      </SettingsGroup>
      <details className="settings-advanced"><summary>Popup appearance</summary><SettingsGroup>
        {settings.writingPopupAnchor === "fixed" ? (
          <SettingRow label="Fixed position" description="Top-left corner of the popup, in pixels.">
            <div className="popup-geometry">
              <label>X<NumberPreference label="Fixed popup X" min={0} max={4000} value={settings.writingPopupX} onChange={value => save({ writingPopupX: value })} /></label>
              <label>Y<NumberPreference label="Fixed popup Y" min={0} max={4000} value={settings.writingPopupY} onChange={value => save({ writingPopupY: value })} /></label>
            </div>
          </SettingRow>
        ) : null}
        <SettingRow label="Popup size" description="Width and maximum height. The window fits its content.">
          <div className="popup-geometry">
            <label>W<NumberPreference label="Popup width" min={280} max={800} value={settings.writingPopupWidth} onChange={value => save({ writingPopupWidth: value })} /></label>
            <label>H<NumberPreference label="Popup height" min={200} max={800} value={settings.writingPopupHeight} onChange={value => save({ writingPopupHeight: value })} /></label>
          </div>
        </SettingRow>
        <SettingRow label="Editable selected text" description="Show the captured highlight in an editable box before running an action.">
          <Switch checked={settings.writingAllowManualText} label="Editable selected text" onChange={(value) => void save({ writingAllowManualText: value })} />
        </SettingRow>
      </SettingsGroup>
      </details>
      <SettingsGroup header="Actions">
        <div className="writing-preferences-actions">{DEFAULT_WRITING_ACTIONS.map((id) => {
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
        </div><div className="settings-group__footer"><Button compact onClick={() => void save({ enabledWritingActions: [...DEFAULT_WRITING_ACTIONS] })}>Reset actions</Button></div>
      </SettingsGroup>
    </SettingsContent>
  );
}

function AiSection({
  apiKey,
  apiStatus,
  busy,
  setApiKey,
  setApiStatus,
  setBusy,
  setNotice,
  settings,
  save,
}: {
  readonly apiKey: string;
  readonly apiStatus: ApiKeyStatus;
  readonly busy: string | null;
  readonly setApiKey: (value: string) => void;
  readonly setApiStatus: (value: ApiKeyStatus | ((current: ApiKeyStatus) => ApiKeyStatus)) => void;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
}) {
  return (
    <SettingsContent title="AI" subtitle="Kivo sends only the text needed for your request directly to Google Gemini.">
      <SettingsGroup header="Model">
        <SettingRow label="Model" description="Newest text models work automatically — only speech, image, video, and agent models are hidden. Or type any model ID via Custom." stacked>
          <AiModelSelect
            disabled={busy !== null}
            onChange={(aiModel) => {
              if (aiModel == null) return;
              void save({ aiModel })
                .then(() => setApiStatus(current => ({ ...current, connection: "untested" })))
                .catch(() => {});
            }}
            value={settings.aiModel}
          />
        </SettingRow>
        <SettingRow label="Backup model" description="If the primary hits its rate limit, Kivo retries once on the backup before reporting an error." stacked>
          <AiModelSelect
            allowNone
            ariaLabel="Backup AI model"
            disabled={busy !== null}
            excludeId={settings.aiModel}
            onChange={(aiBackupModel) => {
              void save({ aiBackupModel })
                .then(() => setApiStatus(current => ({ ...current, connection: "untested" })))
                .catch(() => {});
            }}
            value={settings.aiBackupModel}
          />
        </SettingRow>
      </SettingsGroup>
      <SettingsGroup header="API key">
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
                .then((status) => {
                  setApiStatus(status);
                  setNotice("Gemini is ready for writing requests.");
                })
                .catch((error: unknown) => {
                  // Never leave the indicator stuck at "testing": a failed
                  // test must land on a terminal connection state so the user
                  // can correct the key and retry instead of looping.
                  const code = error instanceof NativeError ? error.code : "";
                  const message = error instanceof NativeError ? error.message : "Couldn’t connect to Gemini.";
                  setNotice(message);
                  setApiStatus((current) => ({
                    ...current,
                    connection: testFailureConnection(code),
                  }));
                })
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
}

function AboutSection({
  busy,
  context,
  setBusy,
  setNotice,
  setUpdateResult,
  updateResult,
}: {
  readonly busy: string | null;
  readonly context: AppContext;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly setUpdateResult: (value: UpdateResult) => void;
  readonly updateResult: UpdateResult | null;
}) {
  const [installed, setInstalled] = useState(false);
  // Unsigned builds (beta, local) have no trusted updater key, so in-app
  // install always fails: remember the definitive failure and stop offering
  // the button, leaving the manual download link as the path.
  const [installUnsupported, setInstallUnsupported] = useState(false);
  const stableAvailable = updateResult?.available === true && updateResult.channel !== "beta";
  return (
    <SettingsContent title="About" subtitle="A quiet writing and dictation utility for your desktop.">
      <div className="about-lockup"><div className="about-lockup__mark"><Icon name="audio" size={27} /></div><div><h2>Kivo</h2><p>Version {context.version}</p></div></div>
      <SettingsGroup>
        <SettingRow label="Software updates" description={installed ? "Update installed. Restart Kivo to finish." : updateDescription(updateResult)}>
          {installed ? (
            <Button
              compact
              disabled={busy !== null}
              onClick={() => {
                setBusy("restart");
                void nativeBridge.restartApp()
                  .catch(() => setNotice("Kivo couldn’t restart. Quit and reopen it manually."))
                  .finally(() => setBusy(null));
              }}
              tone="primary"
            >{busy === "restart" ? "Restarting…" : "Restart now"}</Button>
          ) : (
            <Button
              compact
              disabled={busy !== null}
              onClick={() => {
                setBusy("updates");
                setNotice(null);
                setInstalled(false);
                void nativeBridge.checkForUpdates()
                  .then(setUpdateResult)
                  .catch((error: unknown) => setNotice(error instanceof NativeError ? error.message : "Kivo couldn’t check for updates right now."))
                  .finally(() => setBusy(null));
              }}
            >{busy === "updates" ? "Checking…" : "Check now"}</Button>
          )}
        </SettingRow>
        {stableAvailable && !installed && !installUnsupported ? (
          <SettingRow label="Install update" description={`Version ${updateResult.availableVersion} can be installed without leaving Kivo.`}>
            <Button
              compact
              disabled={busy !== null}
              onClick={() => {
                setBusy("install");
                setNotice(null);
                void nativeBridge.installUpdate()
                  .then(() => {
                    setInstalled(true);
                    setNotice("Update installed. Restart Kivo to finish.");
                  })
                  .catch((error: unknown) => {
                    const code = error instanceof NativeError ? error.code : "";
                    // No installable update on this build (unsigned/beta):
                    // drop the button instead of looping on the same error.
                    if (code === "update_install_unavailable" || code === "update_not_available") setInstallUnsupported(true);
                    setNotice(error instanceof NativeError ? error.message : "The update couldn’t be installed. Use the download link instead.");
                  })
                  .finally(() => setBusy(null));
              }}
              tone="primary"
            >{busy === "install" ? "Installing…" : "Download and Install"}</Button>
          </SettingRow>
        ) : null}
      </SettingsGroup>
      {updateResult?.available && !installed ? (
        <button className="text-link" onClick={() => void nativeBridge.openExternal(updateResult.downloadUrl ?? "https://github.com/0libote/Kivo/releases").catch(() => setNotice("The releases page couldn’t be opened."))} type="button">{updateResult.channel === "beta" ? "Download the latest beta build from GitHub" : "Download the latest release from GitHub"}</button>
      ) : null}
      <div className="about-links" aria-label="Project links">
        <button className="text-link" type="button" onClick={() => void nativeBridge.openExternal("https://github.com/0libote/Kivo").catch(() => setNotice("The repository could not be opened."))}>Source code on GitHub</button>
        <button className="text-link" type="button" onClick={() => void nativeBridge.openExternal("https://github.com/0libote/Kivo/releases").catch(() => setNotice("The releases page could not be opened."))}>Release notes</button>
      </div>
      <p className="privacy-note">No accounts, analytics, or telemetry. Your text is processed only when you invoke Kivo.</p>
    </SettingsContent>
  );
}

function updateDescription(updateResult: UpdateResult | null): string {
  if (updateResult === null) return "Check manually for a newer version.";
  if (updateResult.available) {
    if (updateResult.channel === "beta") {
      const sha = updateResult.availableSha?.slice(0, 7);
      return sha ? `A newer beta build is available (${sha}).` : "A newer beta build is available.";
    }
    return `Version ${updateResult.availableVersion} is available.`;
  }
  return "Kivo is up to date.";
}

function SettingsContent({ title, subtitle, children }: { readonly title: string; readonly subtitle: string; readonly children: ReactNode }) {
  return <section className="settings-content"><header><h1>{title}</h1><p>{subtitle}</p></header>{children}</section>;
}

function SettingsGroup({ header, children }: { readonly header?: string; readonly children: ReactNode }) {
  return <section className="settings-group">{header ? <h2>{header}</h2> : null}<div className="settings-group__body">{children}</div></section>;
}

function SettingRow({ label, description, children, stacked = false }: { readonly label: string; readonly description?: string; readonly children: ReactNode; readonly stacked?: boolean }) {
  return <div className="setting-row" data-stacked={stacked}><div className="setting-row__label"><strong>{label}</strong>{description ? <span>{description}</span> : null}</div><div className="setting-row__control">{children}</div></div>;
}

function connectionLabel(status: ApiKeyStatus) {
  if (!status.configured) return "Not configured";
  const labels: Record<ApiKeyStatus["connection"], string> = {
    untested: "Not tested", testing: "Testing", connected: "Connected", invalid: "Key not accepted", "rate-limited": "Rate limited", offline: "Offline", model: "Model unavailable",
  };
  return labels[status.connection];
}

/** Map a failed Test connection to a terminal indicator state so the UI
 * never sticks at "testing". Mirrors the native CommandError codes
 * (see GeminiError::code in src-tauri/src/ai/mod.rs and AppCoreError::code
 * in src-tauri/src/commands/mod.rs). Exported for unit tests. */
export function testFailureConnection(code: string): ApiKeyStatus["connection"] {
  if (code === "invalid_api_key" || code === "credential" || code === "ai_not_configured") return "invalid";
  if (code === "model_unavailable" || code === "model_not_found") return "model";
  if (code === "rate_limited") return "rate-limited";
  if (code === "transport" || code === "invalid_response" || code === "api_error" || code === "incomplete" || code === "empty_response") return "offline";
  return "untested";
}

function connectionDescription(status: ApiKeyStatus) {
  if (!status.configured) return "Add a key to connect Kivo to Gemini.";
  if (status.connection === "connected") return "The selected model is ready for writing requests.";
  if (status.connection === "invalid") return "Check the key and save it again.";
  if (status.connection === "model") return "The selected model isn’t available to this key. Pick another model above, then test again.";
  if (status.connection === "rate-limited") return "Gemini is temporarily rate limited. Try again shortly.";
  if (status.connection === "offline") return "Kivo couldn’t reach Gemini. Check your connection.";
  return "Test the saved key with the selected model before using Writing Tools.";
}
