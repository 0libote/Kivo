import { useEffect, useMemo, useState } from "react";
import { DictationPractice } from "../../components/DictationPractice";
import { ModelQueueEditor } from "../settings/ModelQueueEditor";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { formatShortcut } from "../../components/ShortcutRecorder";
import { StatusIndicator } from "../../components/StatusIndicator";
import { normalizeAiProvider, providerDefaultModel } from "../../ai/models";
import { nativeBridge } from "../../platform/native";
import type { AiProviderId, ApiKeyStatus, AppContext, AppSettings, PermissionKind, PermissionStatus } from "../../types";

interface OnboardingWindowProps {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
}

export function OnboardingWindow({ context, settings, updateSettings }: OnboardingWindowProps) {
  const [step, setStep] = useState(0);
  const [permissions, setPermissions] = useState<PermissionStatus[]>([]);
  const [busyPermission, setBusyPermission] = useState<PermissionKind | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [apiStatus, setApiStatus] = useState<ApiKeyStatus>({ configured: false, connection: "untested" });
  const [savingKey, setSavingKey] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const pollPermissions = () => {
      void nativeBridge.getPermissions()
        .then((nextPermissions) => active && setPermissions(nextPermissions))
        .catch(() => {});
    };
    // Load independently so a key failure never misreports permissions.
    void nativeBridge.getPermissions()
      .then((nextPermissions) => active && setPermissions(nextPermissions))
      .catch(() => active && setMessage("Permission status isn’t available right now."));
    void nativeBridge.getApiKeyStatus()
      .then((nextApi) => active && setApiStatus(nextApi))
      .catch(() => active && setMessage("Saved key status isn’t available right now."));
    // Grants happen outside the app (system prompt / System Settings) and
    // the native request returns before the user answers, so re-read on
    // focus and poll while onboarding is open.
    const interval = window.setInterval(pollPermissions, 2500);
    window.addEventListener("focus", pollPermissions);
    return () => {
      active = false;
      window.clearInterval(interval);
      window.removeEventListener("focus", pollPermissions);
    };
  }, []);

  const statusByKind = useMemo(
    () => Object.fromEntries(permissions.map((permission) => [permission.kind, permission])) as Partial<Record<PermissionKind, PermissionStatus>>,
    [permissions],
  );

  async function request(kind: PermissionKind) {
    setBusyPermission(kind);
    setMessage(null);
    try {
      // Windows has no in-app prompt: open the Settings page, then re-read
      // the (possibly changed) state instead of firing a no-op request.
      if (context.platform === "windows") {
        await nativeBridge.openPermissionSettings(kind);
        setPermissions(await nativeBridge.getPermissions());
      } else {
        setPermissions(await nativeBridge.requestPermission(kind));
      }
    } catch {
      setMessage("Permission wasn’t granted. You can open Settings and try again.");
    } finally {
      setBusyPermission(null);
    }
  }

  async function finish() {
    if (finishing) return;
    setFinishing(true);
    setMessage(null);
    try {
      await updateSettings({ onboardingComplete: true });
      await nativeBridge.completeOnboarding();
    } catch {
      setMessage("Setup couldn’t be saved. Please try again.");
      setFinishing(false);
    }
  }

  return (
    <main className="onboarding-window" data-platform={context.platform}>
      <section className="onboarding-panel">
        {step === 0 ? <WelcomeStep onStart={() => setStep(1)} /> : null}
        {step === 1 ? (
          <PermissionsStep
            busyPermission={busyPermission}
            dictationShortcut={settings.dictationShortcut}
            platform={context.platform}
            request={(kind) => void request(kind)}
            setMessage={setMessage}
            statusByKind={statusByKind}
          />
        ) : null}
        {step === 2 ? (
          <DictationStep
            dictationShortcut={settings.dictationShortcut}
            busyPermission={busyPermission}
            platform={context.platform}
            request={(kind) => void request(kind)}
            setMessage={setMessage}
            statusByKind={statusByKind}
          />
        ) : null}
        {step === 3 ? (
          <ApiKeyStep
            apiKey={apiKey}
            apiStatus={apiStatus}
            platform={context.platform}
            savingKey={savingKey}
            setApiKey={setApiKey}
            setApiStatus={setApiStatus}
            setMessage={setMessage}
            setSavingKey={setSavingKey}
            settings={settings}
            updateSettings={updateSettings}
          />
        ) : null}

        {message ? <p aria-live="polite" className="onboarding-message">{message}</p> : null}

        {step > 0 ? (
          <OnboardingFooter
            busy={busyPermission !== null || savingKey || finishing}
            finish={() => void finish()}
            finishing={finishing}
            next={() => setStep((current) => current + 1)}
            prev={() => setStep((current) => current - 1)}
            step={step}
          />
        ) : null}
      </section>
    </main>
  );
}

function WelcomeStep({ onStart }: { readonly onStart: () => void }) {
  return (
    <div className="onboarding-step onboarding-welcome">
      <div className="onboarding-mark"><Icon name="audio" size={32} /></div>
      <p className="onboarding-eyebrow">Welcome to Kivo</p>
      <h1>Write naturally,<br />wherever you work.</h1>
      <p className="onboarding-copy">Dictate into any text field and refine selected writing without leaving the app you’re in.</p>
      <Button onClick={onStart} tone="primary">Get started</Button>
    </div>
  );
}

interface StepPermissionsProps {
  readonly busyPermission: PermissionKind | null;
  readonly dictationShortcut: string;
  readonly platform: AppContext["platform"];
  readonly request: (kind: PermissionKind) => void;
  readonly setMessage: (value: string | null) => void;
  readonly statusByKind: Partial<Record<PermissionKind, PermissionStatus>>;
}

function PermissionsStep({ busyPermission, dictationShortcut, platform, request, setMessage, statusByKind }: StepPermissionsProps) {
  const isMacos = platform === "macos";
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__icon"><Icon name="proofread" size={24} /></div>
      <p className="onboarding-eyebrow">Step 1 of 3</p>
      <h1>Work with text everywhere</h1>
      <p className="onboarding-copy">
        {isMacos
          ? "Accessibility lets Kivo read only the text you select and insert text where your cursor is."
          : "Kivo uses Windows UI Automation to work with the selected text and cursor in your active app."}
      </p>
      {isMacos ? (
        <div className="permission-list">
          <PermissionRow
            busy={busyPermission === "accessibility"}
            label="Accessibility"
            onOpen={() => void nativeBridge.openPermissionSettings("accessibility").catch(() => setMessage("The system settings page couldn’t be opened."))}
            onRequest={() => request("accessibility")}
            status={statusByKind.accessibility?.state ?? "not-determined"}
          />
          <PermissionRow
            busy={busyPermission === "input-monitoring"}
            label={dictationShortcut === "Fn" ? "Fn shortcut monitoring" : "Shortcut monitoring"}
            onOpen={() => void nativeBridge.openPermissionSettings("input-monitoring").catch(() => setMessage("The system settings page couldn’t be opened."))}
            onRequest={() => request("input-monitoring")}
            optional={dictationShortcut !== "Fn"}
            status={statusByKind["input-monitoring"]?.state ?? "not-determined"}
          />
        </div>
      ) : (
        <div className="onboarding-native-note"><Icon name="check" size={17} /><span>No permission prompt is normally required.</span></div>
      )}
    </div>
  );
}

function DictationStep({ busyPermission, platform, request, setMessage, statusByKind, dictationShortcut }: StepPermissionsProps) {
  const showSpeechRecognition = platform === "macos" && statusByKind["speech-recognition"]?.state !== "unavailable";
  const openSettings = (kind: PermissionKind) => {
    void nativeBridge.openPermissionSettings(kind).catch(() => setMessage("The system settings page couldn’t be opened."));
  };
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__icon"><Icon name="microphone" size={25} /></div>
      <p className="onboarding-eyebrow">Step 2 of 3</p>
      <h1>Dictation uses system speech</h1>
      <p className="onboarding-copy">Kivo needs microphone access while you hold the dictation shortcut. Audio is handled by the operating system and is never sent to your AI provider or a server operated by Kivo.</p>
      <div className="permission-list">
        <PermissionRow
          busy={busyPermission === "microphone"}
          label="Microphone"
          onOpen={() => openSettings("microphone")}
          onRequest={() => request("microphone")}
          status={statusByKind.microphone?.state ?? "not-determined"}
        />
        {showSpeechRecognition ? (
          <PermissionRow
            busy={busyPermission === "speech-recognition"}
            label="Speech Recognition"
            onOpen={() => openSettings("speech-recognition")}
            onRequest={() => request("speech-recognition")}
            status={statusByKind["speech-recognition"]?.state ?? "not-determined"}
          />
        ) : null}
      </div>
      <DictationPractice platform={platform} shortcut={dictationShortcut} />
      {platform === "windows" ? <p className="onboarding-copy">Use an installed Windows desktop speech language and allow microphone access for desktop apps in Windows Settings.</p> : null}
    </div>
  );
}

interface ApiKeyStepProps {
  readonly apiKey: string;
  readonly apiStatus: ApiKeyStatus;
  readonly platform: AppContext["platform"];
  readonly savingKey: boolean;
  readonly setApiKey: (value: string) => void;
  readonly setApiStatus: (value: ApiKeyStatus) => void;
  readonly setMessage: (value: string | null) => void;
  readonly setSavingKey: (value: boolean) => void;
  readonly settings: AppSettings;
  readonly updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
}

function ApiKeyStep(props: ApiKeyStepProps) {
  const { apiKey, apiStatus, platform, savingKey, setApiKey, setApiStatus, setMessage, setSavingKey, settings, updateSettings } = props;
  const provider = normalizeAiProvider(settings.aiProvider);
  const keyLabel = onboardingKeyLabel(provider);
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__icon"><Icon name="spark" size={24} /></div>
      <p className="onboarding-eyebrow">Step 3 of 3 · Optional</p>
      <h1>Add AI when you’re ready</h1>
      <p className="onboarding-copy">Writing Tools and optional dictation cleanup use an AI provider. Plain dictation works without one, so you can finish setup now and connect a provider later.</p>
      <div className="onboarding-model">
        <label className="onboarding-model__label" htmlFor="onboarding-ai-provider">Provider</label>
        <select
          aria-label="AI provider"
          id="onboarding-ai-provider"
          onChange={(event) => {
            const aiProvider = normalizeAiProvider(event.target.value);
            if (aiProvider === provider) return;
            void updateSettings({ aiProvider })
              .then(() => nativeBridge.getApiKeyStatus().then(setApiStatus).catch(() => {}))
              .catch(() => setMessage("The provider couldn’t be saved."));
          }}
          value={provider}
        >
          <option value="gemini">Gemini</option>
          <option value="zen">OpenCode Zen</option>
          <option value="go">OpenCode Go</option>
          <option value="custom">Custom (OpenAI-compatible)</option>
        </select>
      </div>
      <div className="onboarding-key">
        {apiStatus.configured ? (
          <StatusIndicator label={apiConnectionLabel(apiStatus.connection)} state={apiStatus.connection} />
        ) : (
          <div className="onboarding-key__input">
            <input aria-label={keyLabel} autoComplete="off" onChange={(event) => setApiKey(event.target.value)} placeholder={keyLabel} spellCheck={false} type="password" value={apiKey} />
            <Button
              compact
              disabled={apiKey.trim().length < 8 || savingKey}
              onClick={() => {
                setSavingKey(true);
                void nativeBridge.saveApiKey(apiKey.trim()).then((status) => {
                  setApiStatus(status);
                  setApiKey("");
                }).catch(() => setMessage("The API key couldn’t be saved securely."))
                  .finally(() => setSavingKey(false));
              }}
            >{savingKey ? "Saving…" : "Save"}</Button>
          </div>
        )}
      </div>
      <div className="onboarding-model">
        <span className="onboarding-model__label" id="onboarding-ai-models-label">Main model + fallbacks</span>
        <ModelQueueEditor
          provider={provider}
          onChange={(aiModels) => {
            void updateSettings({ aiModels }).catch(() => setMessage("The models couldn’t be saved."));
          }}
          value={settings.aiModels}
        />
        <p className="onboarding-copy onboarding-model__note">The first row handles normal requests; fallbacks are tried only if it fails{provider === "custom" ? " — pull a model first (e.g. `ollama pull " + providerDefaultModel(provider) + "`)" : ""}.</p>
      </div>
      <div className="shortcut-demo">
        <ShortcutSummary label="Dictate" platform={platform} shortcut={settings.dictationShortcut} />
        <ShortcutSummary label="Writing Tools" platform={platform} shortcut={settings.writingShortcut} />
      </div>
    </div>
  );
}

function apiConnectionLabel(connection: ApiKeyStatus["connection"]): string {
  switch (connection) {
    case "connected": return "API key saved";
    case "testing": return "Testing key…";
    case "invalid": return "Key not accepted";
    case "model": return "Model unavailable — pick another in Settings → AI";
    case "rate-limited": return "Rate limited — try again shortly";
    case "offline": return "Offline — key saved but not verified";
    case "blocked": return "Provider rejected the request — check Settings → AI for details";
    case "untested": return "API key saved — use Test connection in Settings → AI to verify";
  }
}

function onboardingKeyLabel(provider: AiProviderId): string {
  if (provider === "custom") return "API key (optional for local servers)";
  if (provider === "gemini") return "Google AI Studio API key";
  return "OpenCode API key";
}

function OnboardingFooter({ step, prev, next, finish, busy, finishing }: { readonly step: number; readonly prev: () => void; readonly next: () => void; readonly finish: () => void; readonly busy: boolean; readonly finishing: boolean }) {
  const isLast = step >= 3;
  return (
    <footer className="onboarding-footer">
      <button className="onboarding-back" disabled={busy} onClick={prev} type="button">Back</button>
      <div className="onboarding-progress" aria-label={`Onboarding step ${step} of 3`}>
        {[1, 2, 3].map((value) => <span data-active={value === step} key={value} />)}
      </div>
      {isLast ? <Button disabled={busy} onClick={finish} tone="primary">{finishing ? "Finishing…" : "Finish setup"}</Button> : <Button disabled={busy} onClick={next} tone="primary">Continue</Button>}
    </footer>
  );
}

function PermissionRow({ label, status, busy, optional, onRequest, onOpen }: {
  readonly label: string;
  readonly status: PermissionStatus["state"];
  readonly busy: boolean;
  readonly optional?: boolean;
  readonly onRequest: () => void;
  readonly onOpen: () => void;
}) {
  const granted = status === "granted";
  const denied = status === "denied";
  return (
    <div className="permission-row">
      <StatusIndicator label={`${label}${optional ? " · Optional" : ""}`} state={status} />
      {granted ? (
        <span className="permission-row__granted"><Icon name="check" size={15} />Allowed</span>
      ) : (
        <button className="permission-row__action" disabled={busy} onClick={denied ? onOpen : onRequest} type="button">
          {permissionActionLabel(busy, denied)}
        </button>
      )}
    </div>
  );
}

function permissionActionLabel(busy: boolean, denied: boolean): string {
  if (busy) return "Waiting…";
  if (denied) return "Open Settings";
  return "Allow";
}

function ShortcutSummary({ label, shortcut, platform }: { readonly label: string; readonly shortcut: string; readonly platform: AppContext["platform"] }) {
  return (
    <div className="shortcut-summary"><span>{label}</span><div>{formatShortcut(shortcut, platform).map((key) => <kbd key={key}>{key}</kbd>)}</div></div>
  );
}
