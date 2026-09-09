import { useEffect, useMemo, useState } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { formatShortcut } from "../../components/ShortcutRecorder";
import { StatusIndicator } from "../../components/StatusIndicator";
import { nativeBridge } from "../../platform/native";
import type { ApiKeyStatus, AppContext, AppSettings, PermissionKind, PermissionStatus } from "../../types";

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
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void Promise.all([nativeBridge.getPermissions(), nativeBridge.getApiKeyStatus()])
      .then(([nextPermissions, nextApi]) => {
        if (!active) return;
        setPermissions(nextPermissions);
        setApiStatus(nextApi);
      })
      .catch(() => active && setMessage("Permission status isn’t available right now."));
    return () => {
      active = false;
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
      setPermissions(await nativeBridge.requestPermission(kind));
    } catch {
      setMessage("Permission wasn’t granted. You can open Settings and try again.");
    } finally {
      setBusyPermission(null);
    }
  }

  async function finish() {
    setMessage(null);
    try {
      await updateSettings({ onboardingComplete: true });
      await nativeBridge.completeOnboarding();
    } catch {
      setMessage("Setup couldn’t be saved. Please try again.");
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
            statusByKind={statusByKind}
          />
        ) : null}
        {step === 2 ? (
          <DictationStep
            busyPermission={busyPermission}
            platform={context.platform}
            request={(kind) => void request(kind)}
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
          />
        ) : null}

        {message ? <p aria-live="polite" className="onboarding-message">{message}</p> : null}

        {step > 0 ? (
          <OnboardingFooter finish={() => void finish()} next={() => setStep((current) => current + 1)} prev={() => setStep((current) => current - 1)} step={step} />
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
  readonly statusByKind: Partial<Record<PermissionKind, PermissionStatus>>;
}

function PermissionsStep({ busyPermission, dictationShortcut, platform, request, statusByKind }: StepPermissionsProps) {
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
            onOpen={() => void nativeBridge.openPermissionSettings("accessibility")}
            onRequest={() => request("accessibility")}
            status={statusByKind.accessibility?.state ?? "not-determined"}
          />
          <PermissionRow
            busy={busyPermission === "input-monitoring"}
            label={dictationShortcut === "Fn" ? "Fn shortcut monitoring" : "Shortcut monitoring"}
            onOpen={() => void nativeBridge.openPermissionSettings("input-monitoring")}
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

function DictationStep({ busyPermission, platform, request, statusByKind }: Omit<StepPermissionsProps, "dictationShortcut">) {
  const showSpeechRecognition = platform === "macos" && statusByKind["speech-recognition"]?.state !== "unavailable";
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__icon"><Icon name="microphone" size={25} /></div>
      <p className="onboarding-eyebrow">Step 2 of 3</p>
      <h1>Dictation uses system speech</h1>
      <p className="onboarding-copy">Kivo needs microphone access while you hold the dictation shortcut. Audio is handled by the operating system and is never sent to Gemini or a server operated by Kivo.</p>
      <div className="permission-list">
        <PermissionRow
          busy={busyPermission === "microphone"}
          label="Microphone"
          onOpen={() => void nativeBridge.openPermissionSettings("microphone")}
          onRequest={() => request("microphone")}
          status={statusByKind.microphone?.state ?? "not-determined"}
        />
        {showSpeechRecognition ? (
          <PermissionRow
            busy={busyPermission === "speech-recognition"}
            label="Speech Recognition"
            onOpen={() => void nativeBridge.openPermissionSettings("speech-recognition")}
            onRequest={() => request("speech-recognition")}
            status={statusByKind["speech-recognition"]?.state ?? "not-determined"}
          />
        ) : null}
      </div>
      {platform === "windows" ? (
        <p className="onboarding-copy">If dictation can’t start, turn on Online speech recognition under Settings → Privacy &amp; security → Speech.</p>
      ) : null}
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
}

function ApiKeyStep(props: ApiKeyStepProps) {
  const { apiKey, apiStatus, platform, savingKey, setApiKey, setApiStatus, setMessage, setSavingKey, settings } = props;
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__icon"><Icon name="spark" size={24} /></div>
      <p className="onboarding-eyebrow">Step 3 of 3</p>
      <h1>Add Gemini, then you’re ready</h1>
      <p className="onboarding-copy">Your Google AI Studio key is stored by the operating system. It never appears in Kivo’s settings files or logs.</p>
      <div className="onboarding-key">
        {apiStatus.configured ? (
          <StatusIndicator label="API key saved" state="connected" />
        ) : (
          <div className="onboarding-key__input">
            <input aria-label="Google AI Studio API key" autoComplete="off" onChange={(event) => setApiKey(event.target.value)} placeholder="Google AI Studio API key" spellCheck={false} type="password" value={apiKey} />
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
      <div className="shortcut-demo">
        <ShortcutSummary label="Dictate" platform={platform} shortcut={settings.dictationShortcut} />
        <ShortcutSummary label="Writing Tools" platform={platform} shortcut={settings.writingShortcut} />
      </div>
    </div>
  );
}

function OnboardingFooter({ step, prev, next, finish }: { readonly step: number; readonly prev: () => void; readonly next: () => void; readonly finish: () => void }) {
  const isLast = step >= 3;
  return (
    <footer className="onboarding-footer">
      <button className="onboarding-back" onClick={prev} type="button">Back</button>
      <div className="onboarding-progress" aria-label={`Onboarding step ${step} of 3`}>
        {[1, 2, 3].map((value) => <span data-active={value === step} key={value} />)}
      </div>
      {isLast ? <Button onClick={finish} tone="primary">Finish setup</Button> : <Button onClick={next} tone="primary">Continue</Button>}
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
