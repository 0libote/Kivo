import { useEffect, useMemo, useState } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { formatShortcut } from "../../components/ShortcutRecorder";
import { StatusIndicator } from "../../components/StatusIndicator";
import { nativeBridge } from "../../platform/native";
import type { ApiKeyStatus, AppContext, AppSettings, PermissionKind, PermissionStatus } from "../../types";

interface OnboardingWindowProps {
  context: AppContext;
  settings: AppSettings;
  updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
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
      setMessage("Permission wasn’t granted. You can open System Settings and try again.");
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
        {step === 0 ? (
          <div className="onboarding-step onboarding-welcome">
            <div className="onboarding-mark"><Icon name="audio" size={32} /></div>
            <p className="onboarding-eyebrow">Welcome to Kivo</p>
            <h1>Write naturally,<br />wherever you work.</h1>
            <p className="onboarding-copy">Dictate into any text field and refine selected writing without leaving the app you’re in.</p>
            <Button onClick={() => setStep(1)} tone="primary">Get started</Button>
          </div>
        ) : null}

        {step === 1 ? (
          <div className="onboarding-step">
            <div className="onboarding-step__icon"><Icon name="proofread" size={24} /></div>
            <p className="onboarding-eyebrow">Step 1 of 3</p>
            <h1>Work with text everywhere</h1>
            <p className="onboarding-copy">
              {context.platform === "macos"
                ? "Accessibility lets Kivo read only the text you select and insert text where your cursor is."
                : "Kivo uses Windows UI Automation to work with the selected text and cursor in your active app."}
            </p>
            {context.platform === "macos" ? (
              <div className="permission-list">
                <PermissionRow
                  busy={busyPermission === "accessibility"}
                  label="Accessibility"
                  onOpen={() => void nativeBridge.openPermissionSettings("accessibility")}
                  onRequest={() => void request("accessibility")}
                  status={statusByKind.accessibility?.state ?? "not-determined"}
                />
                <PermissionRow
                  busy={busyPermission === "input-monitoring"}
                  label={settings.dictationShortcut === "Fn" ? "Fn shortcut monitoring" : "Shortcut monitoring"}
                  onOpen={() => void nativeBridge.openPermissionSettings("input-monitoring")}
                  onRequest={() => void request("input-monitoring")}
                  optional={settings.dictationShortcut !== "Fn"}
                  status={statusByKind["input-monitoring"]?.state ?? "not-determined"}
                />
              </div>
            ) : (
              <div className="onboarding-native-note"><Icon name="check" size={17} /><span>No permission prompt is normally required.</span></div>
            )}
          </div>
        ) : null}

        {step === 2 ? (
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
                onRequest={() => void request("microphone")}
                status={statusByKind.microphone?.state ?? "not-determined"}
              />
              {context.platform === "macos" && statusByKind["speech-recognition"]?.state !== "unavailable" ? (
                <PermissionRow
                  busy={busyPermission === "speech-recognition"}
                  label="Speech Recognition"
                  onOpen={() => void nativeBridge.openPermissionSettings("speech-recognition")}
                  onRequest={() => void request("speech-recognition")}
                  status={statusByKind["speech-recognition"]?.state ?? "not-determined"}
                />
              ) : null}
            </div>
          </div>
        ) : null}

        {step === 3 ? (
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
              <ShortcutSummary label="Dictate" platform={context.platform} shortcut={settings.dictationShortcut} />
              <ShortcutSummary label="Writing Tools" platform={context.platform} shortcut={settings.writingShortcut} />
            </div>
          </div>
        ) : null}

        {message ? <p aria-live="polite" className="onboarding-message">{message}</p> : null}

        {step > 0 ? (
          <footer className="onboarding-footer">
            <button className="onboarding-back" onClick={() => setStep((current) => current - 1)} type="button">Back</button>
            <div className="onboarding-progress" aria-label={`Onboarding step ${step} of 3`}>
              {[1, 2, 3].map((value) => <span data-active={value === step} key={value} />)}
            </div>
            {step < 3 ? <Button onClick={() => setStep((current) => current + 1)} tone="primary">Continue</Button> : <Button onClick={() => void finish()} tone="primary">Finish setup</Button>}
          </footer>
        ) : null}
      </section>
    </main>
  );
}

function PermissionRow({ label, status, busy, optional, onRequest, onOpen }: {
  label: string;
  status: PermissionStatus["state"];
  busy: boolean;
  optional?: boolean;
  onRequest: () => void;
  onOpen: () => void;
}) {
  const granted = status === "granted";
  const denied = status === "denied";
  return (
    <div className="permission-row">
      <StatusIndicator label={`${label}${optional ? " · Optional" : ""}`} state={status} />
      {granted ? <span className="permission-row__granted"><Icon name="check" size={15} />Allowed</span> : (
        <button className="permission-row__action" disabled={busy} onClick={denied ? onOpen : onRequest} type="button">{busy ? "Waiting…" : denied ? "Open Settings" : "Allow"}</button>
      )}
    </div>
  );
}

function ShortcutSummary({ label, shortcut, platform }: { label: string; shortcut: string; platform: AppContext["platform"] }) {
  return (
    <div className="shortcut-summary"><span>{label}</span><div>{formatShortcut(shortcut, platform).map((key, index) => <kbd key={`${key}-${index}`}>{key}</kbd>)}</div></div>
  );
}
