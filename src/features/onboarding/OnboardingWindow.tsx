import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { motion, useReducedMotion } from "motion/react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { normalizeAiProvider, providerDefaultModel } from "../../ai/models";
import { DictationPractice } from "../../components/DictationPractice";
import { Icon } from "../../components/Icon";
import { StatusIndicator } from "../../components/StatusIndicator";
import { formatShortcut } from "../../components/shortcut";
import { nativeBridge } from "../../platform/native";
import type {
  AiProviderId,
  ApiKeyStatus,
  AppContext,
  AppSettings,
  PermissionKind,
  PermissionStatus,
} from "../../types";
import { ModelQueueEditor } from "../settings/ModelQueueEditor";

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
  const [apiStatus, setApiStatus] = useState<ApiKeyStatus>({
    configured: false,
    connection: "untested",
  });
  const [savingKey, setSavingKey] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const pollPermissions = () => {
      void nativeBridge
        .getPermissions()
        .then((nextPermissions) => active && setPermissions(nextPermissions))
        .catch(() => {});
    };
    // Load independently so a key failure never misreports permissions.
    void nativeBridge
      .getPermissions()
      .then((nextPermissions) => active && setPermissions(nextPermissions))
      .catch(() => active && setMessage("Permission status isn’t available right now."));
    void nativeBridge
      .getApiKeyStatus()
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
    () =>
      Object.fromEntries(permissions.map((permission) => [permission.kind, permission])) as Partial<
        Record<PermissionKind, PermissionStatus>
      >,
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
    <main data-platform={context.platform} {...stylex.props(styles.window)}>
      <section {...stylex.props(styles.panel)}>
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

        {message ? (
          <p aria-live="polite" {...stylex.props(styles.message)}>
            {message}
          </p>
        ) : null}

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

function StepFrame({
  children,
  welcome,
}: {
  readonly children: ReactNode;
  readonly welcome?: boolean;
}) {
  const reduceMotion = useReducedMotion();
  return (
    <motion.div
      animate={{ opacity: 1, x: 0 }}
      initial={reduceMotion ? false : { opacity: 0, x: 6 }}
      transition={reduceMotion ? { duration: 0 } : { duration: 0.18, ease: "easeOut" }}
      {...stylex.props(styles.step, welcome && styles.welcome)}
    >
      {children}
    </motion.div>
  );
}

function WelcomeStep({ onStart }: { readonly onStart: () => void }) {
  return (
    <StepFrame welcome>
      <div {...stylex.props(styles.mark)}>
        <Icon name="audio" size={32} />
      </div>
      <p {...stylex.props(styles.eyebrow)}>Welcome to Kivo</p>
      <h1 {...stylex.props(styles.title)}>
        Write naturally,
        <br />
        wherever you work.
      </h1>
      <p {...stylex.props(styles.copy)}>
        Dictate into any text field and refine selected writing without leaving the app you’re in.
      </p>
      <Button label="Get started" onClick={onStart} variant="primary" />
    </StepFrame>
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

function PermissionsStep({
  busyPermission,
  dictationShortcut,
  platform,
  request,
  setMessage,
  statusByKind,
}: StepPermissionsProps) {
  const isMacos = platform === "macos";
  return (
    <StepFrame>
      <div {...stylex.props(styles.stepIcon)}>
        <Icon name="proofread" size={24} />
      </div>
      <p {...stylex.props(styles.eyebrow)}>Step 1 of 3</p>
      <h1 {...stylex.props(styles.title)}>Work with text everywhere</h1>
      <p {...stylex.props(styles.copy)}>
        {isMacos
          ? "Accessibility lets Kivo read only the text you select and insert text where your cursor is."
          : "Kivo uses Windows UI Automation to work with the selected text and cursor in your active app."}
      </p>
      {isMacos ? (
        <div {...stylex.props(styles.permissionList)}>
          <PermissionRow
            busy={busyPermission === "accessibility"}
            label="Accessibility"
            onOpen={() =>
              void nativeBridge
                .openPermissionSettings("accessibility")
                .catch(() => setMessage("The system settings page couldn’t be opened."))
            }
            onRequest={() => request("accessibility")}
            status={statusByKind.accessibility?.state ?? "not-determined"}
          />
          <PermissionRow
            busy={busyPermission === "input-monitoring"}
            label={dictationShortcut === "Fn" ? "Fn shortcut monitoring" : "Shortcut monitoring"}
            onOpen={() =>
              void nativeBridge
                .openPermissionSettings("input-monitoring")
                .catch(() => setMessage("The system settings page couldn’t be opened."))
            }
            onRequest={() => request("input-monitoring")}
            optional={dictationShortcut !== "Fn"}
            status={statusByKind["input-monitoring"]?.state ?? "not-determined"}
          />
        </div>
      ) : (
        <div {...stylex.props(styles.nativeNote)}>
          <Icon name="check" size={17} />
          <span {...stylex.props(styles.nativeNoteText)}>
            No permission prompt is normally required.
          </span>
        </div>
      )}
    </StepFrame>
  );
}

function DictationStep({
  busyPermission,
  platform,
  request,
  setMessage,
  statusByKind,
  dictationShortcut,
}: StepPermissionsProps) {
  const showSpeechRecognition =
    platform === "macos" && statusByKind["speech-recognition"]?.state !== "unavailable";
  const openSettings = (kind: PermissionKind) => {
    void nativeBridge
      .openPermissionSettings(kind)
      .catch(() => setMessage("The system settings page couldn’t be opened."));
  };
  return (
    <StepFrame>
      <div {...stylex.props(styles.stepIcon)}>
        <Icon name="microphone" size={25} />
      </div>
      <p {...stylex.props(styles.eyebrow)}>Step 2 of 3</p>
      <h1 {...stylex.props(styles.title)}>Dictation uses system speech</h1>
      <p {...stylex.props(styles.copy)}>
        Kivo needs microphone access while you hold the dictation shortcut. Audio is handled by the
        operating system and is never sent to your AI provider or a server operated by Kivo.
      </p>
      <div {...stylex.props(styles.permissionList)}>
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
      <div {...stylex.props(styles.practice)}>
        <DictationPractice platform={platform} shortcut={dictationShortcut} />
      </div>
      {platform === "windows" ? (
        <p {...stylex.props(styles.copy)}>
          Use an installed Windows desktop speech language and allow microphone access for desktop
          apps in Windows Settings.
        </p>
      ) : null}
    </StepFrame>
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
  const {
    apiKey,
    apiStatus,
    platform,
    savingKey,
    setApiKey,
    setApiStatus,
    setMessage,
    setSavingKey,
    settings,
    updateSettings,
  } = props;
  const provider = normalizeAiProvider(settings.aiProvider);
  const keyLabel = onboardingKeyLabel(provider);
  return (
    <StepFrame>
      <div {...stylex.props(styles.stepIcon)}>
        <Icon name="spark" size={24} />
      </div>
      <p {...stylex.props(styles.eyebrow)}>Step 3 of 3 · Optional</p>
      <h1 {...stylex.props(styles.title)}>Add AI when you’re ready</h1>
      <p {...stylex.props(styles.copy)}>
        Writing Tools and optional dictation cleanup use an AI provider. Plain dictation works
        without one, so you can finish setup now and connect a provider later.
      </p>
      <div {...stylex.props(styles.model)}>
        <label {...stylex.props(styles.modelLabel)} htmlFor="onboarding-ai-provider">
          Provider
        </label>
        <select
          aria-label="AI provider"
          id="onboarding-ai-provider"
          onChange={(event) => {
            const aiProvider = normalizeAiProvider(event.target.value);
            if (aiProvider === provider) return;
            void updateSettings({ aiProvider })
              .then(() =>
                nativeBridge
                  .getApiKeyStatus()
                  .then(setApiStatus)
                  .catch(() => {}),
              )
              .catch(() => setMessage("The provider couldn’t be saved."));
          }}
          value={provider}
          {...stylex.props(styles.select)}
        >
          <option value="gemini">Gemini</option>
          <option value="zen">OpenCode Zen</option>
          <option value="go">OpenCode Go</option>
          <option value="custom">Custom (OpenAI-compatible)</option>
        </select>
      </div>
      <div {...stylex.props(styles.key)}>
        {apiStatus.configured ? (
          <StatusIndicator
            label={apiConnectionLabel(apiStatus.connection)}
            state={apiStatus.connection}
          />
        ) : (
          <div {...stylex.props(styles.keyInput)}>
            <input
              aria-label={keyLabel}
              autoComplete="off"
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={keyLabel}
              spellCheck={false}
              type="password"
              value={apiKey}
              {...stylex.props(styles.input)}
            />
            <Button
              isDisabled={apiKey.trim().length < 8 || savingKey}
              label={savingKey ? "Saving…" : "Save"}
              onClick={() => {
                setSavingKey(true);
                void nativeBridge
                  .saveApiKey(apiKey.trim())
                  .then((status) => {
                    setApiStatus(status);
                    setApiKey("");
                  })
                  .catch(() => setMessage("The API key couldn’t be saved securely."))
                  .finally(() => setSavingKey(false));
              }}
              size="sm"
            />
          </div>
        )}
      </div>
      <div {...stylex.props(styles.model)}>
        <span id="onboarding-ai-models-label" {...stylex.props(styles.modelLabel)}>
          Main model + fallbacks
        </span>
        <ModelQueueEditor
          provider={provider}
          onChange={(aiModels) => {
            void updateSettings({ aiModels }).catch(() =>
              setMessage("The models couldn’t be saved."),
            );
          }}
          value={settings.aiModels}
        />
        <p {...stylex.props(styles.copy, styles.modelNote)}>
          The first row handles normal requests; fallbacks are tried only if it fails
          {provider === "custom"
            ? " — pull a model first (e.g. `ollama pull " + providerDefaultModel(provider) + "`)"
            : ""}
          .
        </p>
      </div>
      <div data-testid="shortcut-demo" {...stylex.props(styles.shortcutDemo)}>
        <ShortcutSummary
          label="Dictate"
          platform={platform}
          shortcut={settings.dictationShortcut}
        />
        <ShortcutSummary
          label="Writing Tools"
          platform={platform}
          shortcut={settings.writingShortcut}
        />
      </div>
    </StepFrame>
  );
}

function apiConnectionLabel(connection: ApiKeyStatus["connection"]): string {
  switch (connection) {
    case "connected":
      return "API key saved";
    case "testing":
      return "Testing key…";
    case "invalid":
      return "Key not accepted";
    case "model":
      return "Model unavailable — pick another in Settings → AI";
    case "rate-limited":
      return "Rate limited — try again shortly";
    case "offline":
      return "Offline — key saved but not verified";
    case "blocked":
      return "Provider returned an error — check Settings → AI for details";
    case "untested":
      return "API key saved — use Test connection in Settings → AI to verify";
  }
}

function onboardingKeyLabel(provider: AiProviderId): string {
  if (provider === "custom") return "API key (optional for local servers)";
  if (provider === "gemini") return "Google AI Studio API key";
  return "OpenCode API key";
}

function OnboardingFooter({
  step,
  prev,
  next,
  finish,
  busy,
  finishing,
}: {
  readonly step: number;
  readonly prev: () => void;
  readonly next: () => void;
  readonly finish: () => void;
  readonly busy: boolean;
  readonly finishing: boolean;
}) {
  const isLast = step >= 3;
  const reduceMotion = useReducedMotion();
  return (
    <footer {...stylex.props(styles.footer)}>
      <Button
        isDisabled={busy}
        label="Back"
        onClick={prev}
        size="sm"
        variant="ghost"
        xstyle={styles.back}
      />
      <div aria-label={`Onboarding step ${step} of 3`} {...stylex.props(styles.progress)}>
        {[1, 2, 3].map((value) => {
          const active = value === step;
          return (
            <motion.span
              animate={{ width: active ? 32 : 20 }}
              data-active={active}
              initial={false}
              key={value}
              transition={reduceMotion ? { duration: 0 } : { duration: 0.18, ease: "easeOut" }}
              {...stylex.props(styles.progressDot, active && styles.progressDotActive)}
            />
          );
        })}
      </div>
      {isLast ? (
        <Button
          isDisabled={busy}
          label={finishing ? "Finishing…" : "Finish setup"}
          onClick={finish}
          variant="primary"
          xstyle={styles.footerAction}
        />
      ) : (
        <Button
          isDisabled={busy}
          label="Continue"
          onClick={next}
          variant="primary"
          xstyle={styles.footerAction}
        />
      )}
    </footer>
  );
}

function PermissionRow({
  label,
  status,
  busy,
  optional,
  onRequest,
  onOpen,
}: {
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
    <div {...stylex.props(styles.permissionRow)}>
      <StatusIndicator label={`${label}${optional ? " · Optional" : ""}`} state={status} />
      {granted ? (
        <span {...stylex.props(styles.permissionGranted)}>
          <Icon name="check" size={15} />
          Allowed
        </span>
      ) : (
        <Button
          isDisabled={busy}
          label={permissionActionLabel(busy, denied)}
          onClick={denied ? onOpen : onRequest}
          size="sm"
          variant="secondary"
        />
      )}
    </div>
  );
}

function permissionActionLabel(busy: boolean, denied: boolean): string {
  if (busy) return "Waiting…";
  if (denied) return "Open Settings";
  return "Allow";
}

function ShortcutSummary({
  label,
  shortcut,
  platform,
}: {
  readonly label: string;
  readonly shortcut: string;
  readonly platform: AppContext["platform"];
}) {
  return (
    <div {...stylex.props(styles.shortcutSummary)}>
      <span>{label}</span>
      <div {...stylex.props(styles.shortcutKeys)}>
        {formatShortcut(shortcut, platform).map((key) => (
          <kbd key={key} {...stylex.props(styles.keycap)}>
            {key}
          </kbd>
        ))}
      </div>
    </div>
  );
}

const styles = stylex.create({
  window: {
    display: "grid",
    placeItems: "center",
    width: "100%",
    height: "100%",
    padding: "30px",
    position: "relative",
    overflowY: "auto",
    backgroundColor: "var(--color-background-body)",
  },
  panel: {
    display: "flex",
    flexDirection: "column",
    width: "min(560px, 100%)",
    minHeight: "min(560px, calc(100vh - 60px))",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-container)",
    boxShadow: "var(--shadow-med)",
  },
  step: {
    display: "flex",
    flex: 1,
    flexDirection: "column",
    alignItems: "flex-start",
    paddingBlockStart: "28px",
    paddingBlockEnd: 0,
    paddingInline: "28px",
  },
  welcome: {
    alignItems: "center",
    justifyContent: "center",
    paddingBlockStart: "48px",
    paddingBlockEnd: "58px",
    paddingInline: "40px",
    textAlign: "center",
  },
  mark: {
    display: "grid",
    placeItems: "center",
    width: "70px",
    height: "70px",
    marginBottom: "19px",
    color: "#ffffff",
    backgroundColor: "#242426",
    borderRadius: "var(--radius-element)",
  },
  stepIcon: {
    display: "grid",
    placeItems: "center",
    width: "44px",
    height: "44px",
    marginBottom: "15px",
    color: "var(--color-accent)",
    backgroundColor: "color-mix(in srgb, var(--color-accent) 10%, transparent)",
    borderRadius: "var(--radius-inner)",
  },
  eyebrow: {
    margin: "0 0 8px",
    color: "var(--kivo-text-tertiary)",
    fontSize: "9px",
    fontWeight: 750,
    letterSpacing: "0.13em",
    textTransform: "uppercase",
  },
  title: {
    margin: "0 0 11px",
    fontFamily: "var(--font-family-heading)",
    fontSize: "32px",
    fontWeight: 650,
    lineHeight: 1.16,
    letterSpacing: "-0.045em",
  },
  copy: {
    maxWidth: "430px",
    margin: "0 0 24px",
    color: "var(--color-text-secondary)",
    fontSize: "14px",
    lineHeight: 1.55,
  },
  permissionList: {
    width: "100%",
    overflow: "hidden",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-container)",
  },
  permissionRow: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    minHeight: "58px",
    paddingBlock: "9px",
    paddingInline: "14px",
    borderTopWidth: "1px",
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
    ":first-child": {
      borderTopWidth: 0,
    },
  },
  permissionGranted: {
    display: "flex",
    alignItems: "center",
    gap: "5px",
    color: "var(--color-success)",
    fontSize: "12px",
  },
  nativeNote: {
    display: "flex",
    alignItems: "center",
    gap: "9px",
    padding: "12px",
    color: "var(--color-success)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  nativeNoteText: {
    color: "var(--color-text-primary)",
  },
  key: {
    width: "100%",
    marginBottom: "19px",
  },
  keyInput: {
    display: "flex",
    gap: "7px",
  },
  input: {
    flex: 1,
    minWidth: 0,
    minHeight: "32px",
    paddingBlock: 0,
    paddingInline: "10px",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    boxShadow: "0 1px 2px rgba(17, 19, 23, 0.024)",
    fontSize: "13px",
    ":focus-visible": {
      outline: "2px solid color-mix(in srgb, var(--color-accent) 72%, transparent)",
      outlineOffset: "1px",
    },
    "::placeholder": {
      color: "var(--kivo-text-tertiary)",
    },
  },
  select: {
    width: "100%",
    minHeight: "32px",
    paddingBlock: 0,
    paddingInlineStart: "9px",
    paddingInlineEnd: "28px",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
    boxShadow: "0 1px 2px rgba(17, 19, 23, 0.024)",
    fontSize: "13px",
    ":focus-visible": {
      outline: "2px solid color-mix(in srgb, var(--color-accent) 72%, transparent)",
      outlineOffset: "1px",
    },
  },
  model: {
    display: "grid",
    gap: "8px",
    width: "100%",
    marginBottom: "19px",
    padding: "12px",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-container)",
  },
  modelLabel: {
    fontSize: "13px",
    fontWeight: 600,
  },
  modelNote: {
    margin: 0,
    fontSize: "12px",
  },
  practice: {
    width: "100%",
    marginBlock: "16px",
  },
  shortcutDemo: {
    display: "grid",
    gap: "2px",
    width: "100%",
    overflow: "hidden",
    backgroundColor: "var(--color-border)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-container)",
  },
  shortcutSummary: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    minHeight: "45px",
    paddingBlock: "8px",
    paddingInline: "11px",
    backgroundColor: "var(--color-background-card)",
  },
  shortcutKeys: {
    display: "flex",
    gap: "3px",
  },
  keycap: {
    display: "grid",
    placeItems: "center",
    minWidth: "23px",
    height: "21px",
    paddingInline: "6px",
    fontSize: "11px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderBottomWidth: "2px",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-inner)",
    backgroundColor: "var(--color-background-muted)",
  },
  message: {
    margin: "7px 22px",
    color: "var(--color-error)",
    fontSize: "12px",
  },
  footer: {
    display: "grid",
    gridTemplateColumns: "1fr auto 1fr",
    alignItems: "center",
    minHeight: "68px",
    marginBlockStart: "auto",
    marginInline: "28px",
    paddingBlockStart: "10px",
    paddingBlockEnd: "14px",
    borderTopWidth: "1px",
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
  },
  footerAction: {
    justifySelf: "end",
  },
  back: {
    justifySelf: "start",
    color: "var(--color-text-secondary)",
  },
  progress: {
    display: "flex",
    gap: "6px",
  },
  progressDot: {
    width: "20px",
    height: "4px",
    borderRadius: "var(--radius-full)",
    backgroundColor: "var(--color-border-emphasized)",
  },
  progressDotActive: {
    backgroundColor: "var(--color-accent)",
  },
});
