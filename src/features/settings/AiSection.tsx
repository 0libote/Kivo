import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { normalizeAiProvider } from "../../ai/models";
import { FALLBACK_AI_PROVIDERS } from "../../ai/providers";
import { StatusIndicator } from "../../components/StatusIndicator";
import { nativeBridge } from "../../platform/native";
import {
  type AiProviderId,
  type AiProviderInfo,
  type ApiKeyStatus,
  type AppSettings,
  NativeError,
} from "../../types";
import { testFailureConnection } from "./connection";
import { LocalAiSetup } from "./LocalAiSetup";
import { ModelQueueEditor } from "./ModelQueueEditor";
import { type SaveSettings, SettingRow, SettingsContent, SettingsGroup } from "./settings-layout";
import { styles } from "./settings-styles";
export function AiSection({
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
  const [providers, setProviders] = useState<AiProviderInfo[]>(FALLBACK_AI_PROVIDERS);
  const provider = normalizeAiProvider(settings.aiProvider);
  const info = providers.find((candidate) => candidate.id === provider) ?? FALLBACK_AI_PROVIDERS[0];

  useEffect(() => {
    let active = true;
    void nativeBridge
      .listAiProviders()
      .then((next) => {
        if (active && next.length > 0) setProviders(next);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  async function refreshKeyStatus() {
    try {
      setApiStatus(await nativeBridge.getApiKeyStatus());
    } catch {
      setNotice("Key status isn’t available right now.");
    }
  }

  return (
    <SettingsContent
      title="AI"
      subtitle="Choose the service and models Kivo uses for Writing Tools. Dictation cleanup follows these models unless you set its own under Dictation."
    >
      <SettingsGroup header="Service">
        <SettingRow label="AI service" description={providerBlurb(info)} stacked>
          <select
            aria-label="AI provider"
            disabled={busy !== null}
            onChange={(event) => {
              const aiProvider = normalizeAiProvider(event.target.value);
              if (aiProvider === provider) return;
              setBusy("switch-provider");
              setNotice(null);
              void save({ aiProvider })
                .then(() => refreshKeyStatus())
                .catch(() => {})
                .finally(() => setBusy(null));
            }}
            value={provider}
            {...stylex.props(styles.field)}
          >
            {providers.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.label}
              </option>
            ))}
          </select>
        </SettingRow>
        {provider === "custom" ? (
          <>
            <SettingRow
              label="Server URL"
              description="The address of your OpenAI-compatible API. Use this for Ollama, LM Studio, or a hosted server."
              stacked
            >
              <div {...stylex.props(styles.keyEditor)}>
                <input
                  aria-label="Custom base URL"
                  autoCapitalize="none"
                  autoComplete="off"
                  defaultValue={settings.aiCustomBaseUrl ?? ""}
                  key={provider}
                  onBlur={(event) => {
                    const raw = event.target.value.trim();
                    const aiCustomBaseUrl = raw === "" ? null : raw;
                    if (aiCustomBaseUrl !== settings.aiCustomBaseUrl) {
                      void save({ aiCustomBaseUrl }).catch(() =>
                        setNotice("The base URL couldn’t be saved."),
                      );
                    }
                  }}
                  placeholder={info.defaultBaseUrl ?? "http://localhost:11434/v1"}
                  spellCheck={false}
                  type="url"
                  {...stylex.props(styles.field, styles.growField)}
                />
              </div>
            </SettingRow>
            <SettingRow
              label="Local servers"
              description="Kivo checks this computer for a running Ollama, LM Studio, or llama.cpp server."
              stacked
            >
              <LocalAiSetup disabled={busy !== null} save={save} settings={settings} />
            </SettingRow>
          </>
        ) : null}
      </SettingsGroup>
      <SettingsGroup header="Connection">
        <SettingRow label={keyLabel(info)} description={keyDescription(info, apiStatus)} stacked>
          <div {...stylex.props(styles.keyEditor)}>
            <input
              aria-label={keyLabel(info)}
              autoCapitalize="none"
              autoComplete="off"
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={apiStatus.configured ? "Enter a replacement key" : keyPlaceholder(info)}
              spellCheck={false}
              type="password"
              value={apiKey}
              {...stylex.props(styles.field, styles.growField)}
            />
            <Button
              isDisabled={apiKey.trim().length < 8 || busy !== null}
              label={busy === "save-key" ? "Saving…" : "Save key"}
              onClick={() => {
                setBusy("save-key");
                setNotice(null);
                void nativeBridge
                  .saveApiKey(apiKey.trim())
                  .then((status) => {
                    setApiStatus(status);
                    setApiKey("");
                    setNotice("API key saved securely.");
                  })
                  .catch((error: unknown) =>
                    setNotice(
                      error instanceof NativeError
                        ? error.message
                        : "The API key couldn’t be saved.",
                    ),
                  )
                  .finally(() => setBusy(null));
              }}
              size="sm"
              variant="primary"
            />
          </div>
        </SettingRow>
        <SettingRow label="Status" description={connectionDescription(info, apiStatus)}>
          <StatusIndicator label={connectionLabel(apiStatus)} state={apiStatus.connection} />
        </SettingRow>
        <div {...stylex.props(styles.groupFooter, styles.groupFooterSplit)}>
          <Button
            isDisabled={(!apiStatus.configured && !info.keyOptional) || busy !== null}
            label={busy === "test-key" ? "Testing…" : "Test connection"}
            onClick={() => {
              setBusy("test-key");
              setNotice(null);
              setApiStatus((current) => ({ ...current, connection: "testing" }));
              void nativeBridge
                .testApiKey()
                .then((status) => {
                  setApiStatus(status);
                  setNotice(`${info.label} is ready for writing requests.`);
                })
                .catch((error: unknown) => {
                  const code = error instanceof NativeError ? error.code : "";
                  const message =
                    error instanceof NativeError
                      ? error.message
                      : `Couldn’t connect to ${info.label}.`;
                  setNotice(message);
                  setApiStatus((current) => ({
                    ...current,
                    connection: testFailureConnection(code),
                  }));
                })
                .finally(() => setBusy(null));
            }}
            size="sm"
            variant="secondary"
          />
          {apiStatus.configured ? (
            <Button
              label="Remove key"
              onClick={() => {
                setBusy("clear-key");
                setNotice(null);
                void nativeBridge
                  .clearApiKey()
                  .then(setApiStatus)
                  .catch((error: unknown) =>
                    setNotice(
                      error instanceof NativeError
                        ? error.message
                        : "The API key couldn’t be removed.",
                    ),
                  )
                  .finally(() => setBusy(null));
              }}
              size="sm"
              variant="destructive"
            />
          ) : null}
        </div>
        {info.keyUrl ? (
          <div {...stylex.props(styles.groupFooter)}>
            <button
              onClick={() =>
                void nativeBridge
                  .openExternal(info.keyUrl as string)
                  .catch(() => setNotice(`${info.label} couldn’t be opened.`))
              }
              type="button"
              {...stylex.props(styles.textLink)}
            >
              {keyLinkLabel(info)}
            </button>
          </div>
        ) : null}
      </SettingsGroup>
      <SettingsGroup header="Models">
        <SettingRow
          label="Models, in order"
          description="Kivo tries the primary model first. If it fails, Kivo tries each fallback in order."
          stacked
        >
          <ModelQueueEditor
            disabled={busy !== null}
            provider={provider}
            onChange={(aiModels) => {
              void save({ aiModels })
                .then(() => setApiStatus((current) => ({ ...current, connection: "untested" })))
                .catch(() => {});
            }}
            value={settings.aiModels}
          />
        </SettingRow>
        {!info.supportsLinkSummary ? (
          <p {...stylex.props(styles.note)}>
            Link summaries require Gemini. {info.label} can still summarize selected text.
          </p>
        ) : null}
      </SettingsGroup>
      <SettingsGroup header="Response">
        <SettingRow
          label="Thinking level"
          description="Fast is best for everyday writing. Higher levels can help difficult rewrites, but take longer and may use more quota."
          stacked
        >
          <select
            aria-label="AI reasoning mode"
            disabled={busy !== null}
            onChange={(event) => {
              const aiReasoningMode = event.target.value as AppSettings["aiReasoningMode"];
              void save({ aiReasoningMode });
            }}
            value={settings.aiReasoningMode}
            {...stylex.props(styles.field)}
          >
            <option value="fast">Fast — recommended</option>
            <option value="balanced">Balanced — more thorough</option>
            <option value="deep">Deep — slowest</option>
          </select>
        </SettingRow>
      </SettingsGroup>
      {info.testUsesQuota ? (
        <p {...stylex.props(styles.note)}>
          Testing the connection sends one short request and may use a small amount of quota.
        </p>
      ) : null}
      {!info.keyUrl ? (
        <p {...stylex.props(styles.note)}>
          No key needed for a local server. Pull a model first — e.g.{" "}
          <code>ollama pull {info.defaultModel}</code> — then Refresh the model list.
        </p>
      ) : null}
    </SettingsContent>
  );
}

function providerBlurb(info: AiProviderInfo): string {
  switch (info.id as AiProviderId) {
    case "zen":
      return "OpenCode’s pay-as-you-go service. Model prices appear below.";
    case "go":
      return "Uses your OpenCode Go subscription. OpenCode designs Go for coding-agent traffic; GLM-5.3 Flash is the recommended model.";
    case "custom":
      return "Connect to Ollama, LM Studio, or another OpenAI-compatible server.";
    default:
      return "Connects directly to Google and supports summaries of highlighted links.";
  }
}

function keyLabel(info: AiProviderInfo): string {
  if (info.id === "custom") return "Custom endpoint API key (optional)";
  if (info.id === "zen" || info.id === "go") return "OpenCode API key";
  return "Google AI Studio API key";
}

function keyPlaceholder(info: AiProviderInfo): string {
  if (info.id === "custom") return "Enter API key (leave empty for local servers)";
  if (info.id === "zen") return "Enter Zen API key";
  if (info.id === "go") return "Enter Go API key";
  return "Enter API key";
}

function keyDescription(info: AiProviderInfo, status: ApiKeyStatus): string {
  if (status.configured) return "A key is stored securely by the operating system.";
  if (info.id === "custom")
    return "Optional for local servers like Ollama; required for hosted endpoints.";
  if (info.id === "zen")
    return "Pay-as-you-go credits from opencode.ai/auth. Required for writing actions and optional dictation cleanup.";
  if (info.id === "go") return "Your $10/month Go subscription key from opencode.ai/auth.";
  return "Required for writing actions and optional dictation cleanup.";
}

function keyLinkLabel(info: AiProviderInfo): string {
  if (info.id === "zen" || info.id === "go")
    return "Get an API key from OpenCode (Zen credits or Go subscription)";
  return "Get an API key from Google AI Studio";
}

function connectionLabel(status: ApiKeyStatus) {
  if (!status.configured) return "Not configured";
  const labels: Record<ApiKeyStatus["connection"], string> = {
    untested: "Not tested",
    testing: "Testing",
    connected: "Connected",
    invalid: "Key not accepted",
    "rate-limited": "Rate limited",
    offline: "Offline",
    model: "Model unavailable",
    blocked: "Provider error",
  };
  return labels[status.connection];
}

function connectionDescription(info: AiProviderInfo, status: ApiKeyStatus) {
  if (!status.configured && !info.keyOptional) return `Add a key to connect Kivo to ${info.label}.`;
  if (!status.configured) return `Add a key, or leave it empty for a local server.`;
  if (status.connection === "connected") return "The selected model is ready for writing requests.";
  if (status.connection === "invalid") return "Check the key and save it again.";
  if (status.connection === "model")
    return "A queued model isn’t available to this key. Pick another model above, then test again.";
  if (status.connection === "rate-limited")
    return `${info.label} is temporarily rate limited. Try again shortly.`;
  if (status.connection === "blocked")
    return `${info.label} responded with an error. Read the message above for the provider's details.`;
  if (status.connection === "offline")
    return `Kivo couldn’t reach ${info.label}. Check your connection.`;
  return "Test the saved key with the selected model before using Writing Tools.";
}
