import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  fallbackAiModels,
  migrateAiModels,
  normalizeAiModelList,
  normalizeAiProvider,
} from "../ai/models";
import {
  type AiModelInfo,
  type AiProviderId,
  type AiProviderInfo,
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  type DictationSnapshot,
  type MicrophoneDevice,
  NativeError,
  type NativeErrorShape,
  type PermissionKind,
  type PermissionStatus,
  type Platform,
  type SelectionContext,
  type SpeechLanguage,
  type Surface,
  type WritingRequest,
  type WritingResponse,
  defaultSettings,
} from "../types";

type NativeEventMap = {
  "show-about": null;
  "recovery-changed": null;
  "pause-changed": boolean;
  "dictation-state": DictationSnapshot;
  "dictation-level": number;
  "writing-context": SelectionContext;
  "writing-error": NativeErrorShape;
  "permission-status-changed": PermissionStatus[];
  "settings-changed": AppSettings;
};

export interface UpdateResult {
  currentVersion: string;
  availableVersion?: string;
  available: boolean;
  downloadUrl?: string;
  channel?: "stable" | "beta";
  currentSha?: string;
  availableSha?: string;
}

export interface NativeBridge {
  readonly isNative: boolean;
  getContext(): Promise<AppContext>;
  getSettings(): Promise<AppSettings>;
  getDictationRecovery(): Promise<string | null>;
  clearDictationRecovery(): Promise<void>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  getPermissions(): Promise<PermissionStatus[]>;
  requestPermission(kind: PermissionKind): Promise<PermissionStatus[]>;
  openPermissionSettings(kind: PermissionKind): Promise<void>;
  resetPermissionGrants(): Promise<PermissionStatus[]>;
  listMicrophones(): Promise<MicrophoneDevice[]>;
  listSpeechLanguages(): Promise<SpeechLanguage[]>;
  listAiProviders(): Promise<AiProviderInfo[]>;
  listAiModels(): Promise<AiModelInfo[]>;
  getApiKeyStatus(): Promise<ApiKeyStatus>;
  saveApiKey(apiKey: string): Promise<ApiKeyStatus>;
  clearApiKey(): Promise<ApiKeyStatus>;
  testApiKey(): Promise<ApiKeyStatus>;
  startDictation(): Promise<void>;
  stopDictation(): Promise<void>;
  cancelDictation(): Promise<void>;
  retryDictation(): Promise<void>;
  getWritingContext(): Promise<SelectionContext>;
  runWritingAction(request: WritingRequest): Promise<WritingResponse>;
  replaceWritingResult(text: string): Promise<void>;
  copyText(text: string): Promise<void>;
  closeSurface(surface: Surface): Promise<void>;
  showSurface(surface: Surface): Promise<void>;
  setSurfaceMode(surface: "writing-tools", mode: "menu" | "custom" | "summary" | "processing" | "result" | "error", height?: number): Promise<void>;
  completeOnboarding(): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
  checkForUpdates(): Promise<UpdateResult>;
  installUpdate(): Promise<void>;
  restartApp(): Promise<void>;
  openExternal(url: string): Promise<void>;
  on<K extends keyof NativeEventMap>(event: K, handler: (payload: NativeEventMap[K]) => void): Promise<UnlistenFn>;
}

export function detectedPlatform(): Platform {
  if (/Windows/i.test(navigator.userAgent)) return "windows";
  if (/Linux|X11/i.test(navigator.userAgent)) return "linux";
  return "macos";
}

function surfaceFromLabel(label: string | undefined): Surface {
  const querySurface = new URLSearchParams(window.location.search).get("surface");
  const candidate = querySurface ?? label;
  switch (candidate) {
    case "flow-bar":
    case "writing-tools":
    case "settings":
    case "onboarding":
    case "gallery":
      return candidate;
    default:
      return "settings";
  }
}

function normalizeError(error: unknown): NativeError {
  if (error instanceof NativeError) return error;
  if (typeof error === "object" && error !== null && "message" in error) {
    const shape = error as Partial<NativeErrorShape>;
    return new NativeError({
      code: shape.code ?? "native-error",
      message: typeof shape.message === "string" ? shape.message : "The operation couldn’t be completed.",
      recoverable: shape.recoverable,
    });
  }
  return new NativeError({
    code: "native-error",
    message: typeof error === "string" ? error : "The operation couldn’t be completed.",
  });
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

class TauriBridge implements NativeBridge {
  readonly isNative = true;

  async getContext(): Promise<AppContext> {
    const label = getCurrentWindow().label;
    const context = await call<Omit<AppContext, "surface">>("get_app_context");
    return { ...context, surface: surfaceFromLabel(label) };
  }

  getSettings = () => call<AppSettings>("get_settings");
  getDictationRecovery = () => call<string | null>("get_dictation_recovery");
  clearDictationRecovery = () => call<void>("clear_dictation_recovery");
  updateSettings = (patch: Partial<AppSettings>) => call<AppSettings>("update_settings", { patch });
  getPermissions = () => call<PermissionStatus[]>("get_permission_statuses");
  requestPermission = (kind: PermissionKind) => call<PermissionStatus[]>("request_permission", { kind });
  openPermissionSettings = (kind: PermissionKind) => call<void>("open_permission_settings", { kind });
  resetPermissionGrants = () => call<PermissionStatus[]>("reset_permission_grants");
  listMicrophones = () => call<MicrophoneDevice[]>("list_microphones");
  listSpeechLanguages = () => call<SpeechLanguage[]>("list_speech_languages");
  listAiProviders = () => call<AiProviderInfo[]>("list_ai_providers");
  listAiModels = () => call<AiModelInfo[]>("list_ai_models");
  getApiKeyStatus = () => call<ApiKeyStatus>("get_api_key_status");
  saveApiKey = (apiKey: string) => call<ApiKeyStatus>("store_api_key", { apiKey });
  clearApiKey = () => call<ApiKeyStatus>("remove_api_key");
  testApiKey = () => call<ApiKeyStatus>("test_api_key");
  startDictation = () => call<void>("start_dictation");
  stopDictation = () => call<void>("stop_dictation");
  cancelDictation = () => call<void>("cancel_dictation");
  retryDictation = () => call<void>("retry_dictation");
  getWritingContext = () => call<SelectionContext>("get_writing_context");
  runWritingAction = (request: WritingRequest) => call<WritingResponse>("run_writing_action", { request });
  replaceWritingResult = (text: string) => call<void>("replace_writing_result", { text });
  copyText = (text: string) => call<void>("copy_text", { text });
  closeSurface = (surface: Surface) => call<void>("close_surface", { surface });
  showSurface = (surface: Surface) => call<void>("show_surface", { surface });
  setSurfaceMode = (surface: "writing-tools", mode: "menu" | "custom" | "summary" | "processing" | "result" | "error", height?: number) => call<void>("set_surface_mode", { surface, mode, height });
  completeOnboarding = () => call<void>("complete_onboarding");
  setPaused = (paused: boolean) => call<void>("set_paused", { paused });
  checkForUpdates = () => call<UpdateResult>("check_for_updates");
  installUpdate = () => call<void>("install_update");
  restartApp = () => call<void>("restart_app");
  openExternal = (url: string) => call<void>("open_external", { url });

  async on<K extends keyof NativeEventMap>(
    event: K,
    handler: (payload: NativeEventMap[K]) => void,
  ): Promise<UnlistenFn> {
    return listen<NativeEventMap[K]>(event, ({ payload }) => handler(payload));
  }
}

type UntypedListener = (payload: unknown) => void;
type ListenerMap = Partial<Record<keyof NativeEventMap, Set<UntypedListener>>>;

class MockBridge implements NativeBridge {
  readonly isNative = false;
  private readonly platform = detectedPlatform();
  private settings: AppSettings;
  private permissions: PermissionStatus[];
  private paused = false;
  private apiKeyStatuses: Record<AiProviderId, ApiKeyStatus> = {
    gemini: { configured: false, connection: "untested" },
    zen: { configured: false, connection: "untested" },
    go: { configured: false, connection: "untested" },
    custom: { configured: false, connection: "untested" },
  };
  private readonly listeners: ListenerMap = {};

  constructor() {
    const stored = window.localStorage.getItem("kivo-dev-settings");
    const parsed = stored ? (JSON.parse(stored) as Partial<AppSettings>) : {};
    this.settings = { ...defaultSettings(this.platform), ...parsed };
    // Pre-queue harnesses stored a single model + backup: fold them into the
    // ordered queue so the selector never starts empty.
    this.settings.aiProvider = normalizeAiProvider(
      typeof parsed.aiProvider === "string" ? parsed.aiProvider : this.settings.aiProvider,
    );
    this.settings.aiModels = normalizeAiModelList(
      this.settings.aiProvider,
      migrateAiModels(parsed),
    );
    // Mirror the native side: only macOS has an in-app consent prompt, so
    // everywhere else the microphone/speech rows read granted (engines
    // assumed present) and input monitoring reads unavailable instead of
    // looping on an ungrantable Allow button. Linux uses the simulated
    // speech engine, which is always present.
    const portable = this.platform !== "macos";
    this.permissions = [
      { kind: "accessibility", state: "not-determined", required: true },
      { kind: "input-monitoring", state: this.platform === "macos" ? "not-determined" : "unavailable", required: false },
      { kind: "microphone", state: portable ? "granted" : "not-determined", required: true },
      { kind: "speech-recognition", state: portable ? "granted" : "not-determined", required: this.platform === "macos" },
    ];
  }

  async getContext(): Promise<AppContext> {
    return {
      platform: this.platform,
      surface: surfaceFromLabel(undefined),
      version: "0.1.0-dev",
      development: true,
      paused: this.paused,
    };
  }

  async getDictationRecovery(): Promise<string | null> { return null; }
  async clearDictationRecovery() { this.emit("recovery-changed", null); }

  async getSettings() {
    return structuredClone(this.settings);
  }

  async updateSettings(patch: Partial<AppSettings>) {
    const previousProvider = this.settings.aiProvider;
    this.settings = { ...this.settings, ...patch };
    // Mirror SettingsPatch: provider-specific IDs must not carry over by default.
    if (patch.aiProvider !== undefined) {
      this.settings.aiProvider = normalizeAiProvider(patch.aiProvider);
      if (this.settings.aiProvider !== previousProvider && patch.aiModels === undefined) {
        this.settings.aiModels = [];
      }
    }
    if (patch.aiModels !== undefined || patch.aiProvider !== undefined) {
      this.settings.aiModels = normalizeAiModelList(
        this.settings.aiProvider,
        this.settings.aiModels ?? [],
      );
    }
    window.localStorage.setItem("kivo-dev-settings", JSON.stringify(this.settings));
    this.emit("settings-changed", structuredClone(this.settings));
    return structuredClone(this.settings);
  }

  async getPermissions() {
    return structuredClone(this.permissions);
  }

  async requestPermission(kind: PermissionKind) {
    await delay(350);
    this.permissions = this.permissions.map((permission) =>
      // Mirror the native side: kinds the OS has no prompt for stay as they
      // are instead of flipping to granted.
      permission.kind === kind && permission.state === "not-determined"
        ? { ...permission, state: "granted" }
        : permission,
    );
    this.emit("permission-status-changed", structuredClone(this.permissions));
    return structuredClone(this.permissions);
  }

  async openPermissionSettings() {
    // Intentional no-op: browser harness has no OS settings screen to open.
  }

  async resetPermissionGrants() {
    // Mirror a TCC reset: entries the OS would forget become grantable again.
    await delay(350);
    this.permissions = this.permissions.map((permission) =>
      permission.state === "unavailable"
        ? permission
        : { ...permission, state: "not-determined" as const },
    );
    this.emit("permission-status-changed", structuredClone(this.permissions));
    return structuredClone(this.permissions);
  }

  async listMicrophones() {
    return [
      { id: "default", name: "System Default", isDefault: true },
      { id: "studio-display", name: "Studio Display Microphone", isDefault: false },
    ];
  }

  async listSpeechLanguages() {
    return [
      { code: "auto", name: "Automatic", installed: true, downloadable: false },
      { code: "en-GB", name: "English (United Kingdom)", installed: true, downloadable: false },
      { code: "en-US", name: "English (United States)", installed: true, downloadable: false },
      { code: "fr-FR", name: "French (France)", installed: false, downloadable: true },
    ];
  }

  async listAiProviders(): Promise<AiProviderInfo[]> {
    return [
      { id: "gemini", label: "Gemini", keyUrl: "https://aistudio.google.com/app/apikey", keyOptional: false, defaultModel: "gemini-3.8-flash", defaultBaseUrl: null, supportsLinkSummary: true, testUsesQuota: true },
      { id: "zen", label: "OpenCode Zen", keyUrl: "https://opencode.ai/auth", keyOptional: false, defaultModel: "gemini-3.8-flash", defaultBaseUrl: null, supportsLinkSummary: false, testUsesQuota: true },
      { id: "go", label: "OpenCode Go", keyUrl: "https://opencode.ai/auth", keyOptional: false, defaultModel: "glm-5.3-flash", defaultBaseUrl: null, supportsLinkSummary: false, testUsesQuota: true },
      { id: "custom", label: "Custom (OpenAI-compatible)", keyUrl: null, keyOptional: true, defaultModel: "llama3.1", defaultBaseUrl: "http://localhost:11434/v1", supportsLinkSummary: false, testUsesQuota: true },
    ];
  }

  async listAiModels(): Promise<AiModelInfo[]> {
    return structuredClone(fallbackAiModels(this.settings.aiProvider));
  }

  private currentKeyStatus(): ApiKeyStatus {
    return { ...this.apiKeyStatuses[this.settings.aiProvider] };
  }

  async getApiKeyStatus() {
    return this.currentKeyStatus();
  }

  async saveApiKey(apiKey: string) {
    await delay(250);
    const provider = this.settings.aiProvider;
    this.apiKeyStatuses[provider] = { configured: apiKey.trim().length > 8, connection: "untested" };
    return this.currentKeyStatus();
  }

  async clearApiKey() {
    this.apiKeyStatuses[this.settings.aiProvider] = { configured: false, connection: "untested" };
    return this.currentKeyStatus();
  }

  async testApiKey() {
    const provider = this.settings.aiProvider;
    this.apiKeyStatuses[provider] = { ...this.apiKeyStatuses[provider], connection: "testing" };
    await delay(700);
    this.apiKeyStatuses[provider] = {
      configured: this.apiKeyStatuses[provider].configured,
      connection: this.apiKeyStatuses[provider].configured ? "connected" : "invalid",
    };
    return this.currentKeyStatus();
  }

  async startDictation() {
    this.emit("dictation-state", { status: "listening", sessionId: crypto.randomUUID() });
  }

  async stopDictation() {
    this.emit("dictation-state", { status: "processing" });
    await delay(700);
    this.emit("dictation-state", { status: "success" });
  }

  async cancelDictation() {
    this.emit("dictation-state", { status: "hidden" });
  }

  async retryDictation() {
    await this.startDictation();
  }

  async getWritingContext() {
    const applicationNames: Record<Platform, string> = {
      macos: "TextEdit",
      windows: "Notepad",
      linux: "Text Editor",
    };
    return {
      hasSelection: true,
      applicationName: applicationNames[this.platform],
      canReplace: true,
      bounds: { x: 480, y: 320, width: 164, height: 22 },
      initialText: "Hello, how are you?",
    };
  }

  async runWritingAction(request: WritingRequest): Promise<WritingResponse> {
    await delay(850);
    if (request.action === "summarize" && request.sourceKind === "link") {
      const text = (request.text ?? "").trim();
      let kind: "website" | "youtube" = "website";
      try {
        if (new URL(text).hostname.includes("youtu")) kind = "youtube";
      } catch {
        // Invalid URLs never reach the mock in production (the popup blocks
        // them); fall through with the default kind rather than throwing a
        // raw TypeError the real backend would never produce.
      }
      return {
        kind: "result",
        text: "This preview demonstrates the summary layout. Link content is retrieved only in the native app.",
        source: { kind, url: request.text! },
        canReplace: false,
      };
    }
    if (request.action === "summarize") {
      return { kind: "result", text: "A short greeting that asks how the other person is doing." };
    }
    if (request.action === "key-points") {
      return { kind: "result", text: "- Opens with a casual greeting\n- Asks how the recipient is doing" };
    }
    return { kind: "replaced" };
  }

  async replaceWritingResult() {
    // Intentional no-op: browser harness previews results instead of replacing text.
  }

  async copyText(text: string) {
    await navigator.clipboard?.writeText(text);
  }

  async closeSurface() {
    // Intentional no-op: browser harness keeps surfaces visible for development.
  }
  async showSurface(surface: Surface) {
    window.location.search = `?surface=${surface}&harness=1`;
  }
  async setSurfaceMode() {
    // Intentional no-op: browser harness does not resize native windows.
  }
  async completeOnboarding() {
    await this.updateSettings({ onboardingComplete: true });
  }
  async setPaused(paused: boolean) {
    this.paused = paused;
    this.emit("pause-changed", paused);
  }
  async checkForUpdates() {
    await delay(450);
    return { currentVersion: "0.1.0-dev", available: false };
  }
  async installUpdate() {
    await delay(800);
  }
  async restartApp() {
    // Intentional no-op: browser harness keeps running for development.
  }
  async openExternal(url: string) {
    window.open(url, "_blank", "noopener,noreferrer");
  }

  async on<K extends keyof NativeEventMap>(
    event: K,
    handler: (payload: NativeEventMap[K]) => void,
  ): Promise<UnlistenFn> {
    const listeners = (this.listeners[event] ??= new Set());
    const listener: UntypedListener = (payload) => handler(payload as NativeEventMap[K]);
    listeners.add(listener);
    return () => listeners.delete(listener);
  }

  private emit<K extends keyof NativeEventMap>(event: K, payload: NativeEventMap[K]) {
    this.listeners[event]?.forEach((listener) => listener(payload));
  }
}

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));
}

export const nativeBridge: NativeBridge = window.__TAURI_INTERNALS__ ? new TauriBridge() : new MockBridge();

export function initialAppContext(): AppContext {
  // Synchronous guess so each window paints on load instead of waiting for
  // the get_app_context round-trip; hydrated with real values right after.
  const platform = detectedPlatform();
  let surface: Surface;
  try {
    surface = surfaceFromLabel(window.__TAURI_INTERNALS__ ? getCurrentWindow().label : undefined);
  } catch {
    surface = surfaceFromLabel(undefined);
  }
  return { platform, surface, version: "", development: false, paused: false };
}

export { surfaceFromLabel };
