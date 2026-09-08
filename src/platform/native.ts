import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
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
}

export interface NativeBridge {
  readonly isNative: boolean;
  getContext(): Promise<AppContext>;
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  getPermissions(): Promise<PermissionStatus[]>;
  requestPermission(kind: PermissionKind): Promise<PermissionStatus[]>;
  openPermissionSettings(kind: PermissionKind): Promise<void>;
  listMicrophones(): Promise<MicrophoneDevice[]>;
  listSpeechLanguages(): Promise<SpeechLanguage[]>;
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
  setSurfaceMode(surface: "writing-tools", mode: "menu" | "chat" | "custom" | "processing" | "result" | "error"): Promise<void>;
  completeOnboarding(): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
  checkForUpdates(): Promise<UpdateResult>;
  openExternal(url: string): Promise<void>;
  on<K extends keyof NativeEventMap>(event: K, handler: (payload: NativeEventMap[K]) => void): Promise<UnlistenFn>;
}

function detectedPlatform(): Platform {
  if (/Windows/i.test(navigator.userAgent)) return "windows";
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
  updateSettings = (patch: Partial<AppSettings>) => call<AppSettings>("update_settings", { patch });
  getPermissions = () => call<PermissionStatus[]>("get_permission_statuses");
  requestPermission = (kind: PermissionKind) => call<PermissionStatus[]>("request_permission", { kind });
  openPermissionSettings = (kind: PermissionKind) => call<void>("open_permission_settings", { kind });
  listMicrophones = () => call<MicrophoneDevice[]>("list_microphones");
  listSpeechLanguages = () => call<SpeechLanguage[]>("list_speech_languages");
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
  setSurfaceMode = (surface: "writing-tools", mode: "menu" | "chat" | "custom" | "processing" | "result" | "error") => call<void>("set_surface_mode", { surface, mode });
  completeOnboarding = () => call<void>("complete_onboarding");
  setPaused = (paused: boolean) => call<void>("set_paused", { paused });
  checkForUpdates = () => call<UpdateResult>("check_for_updates");
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
  private apiKeyStatus: ApiKeyStatus = { configured: false, connection: "untested" };
  private listeners: ListenerMap = {};

  constructor() {
    const stored = window.localStorage.getItem("kivo-dev-settings");
    this.settings = stored
      ? { ...defaultSettings(this.platform), ...(JSON.parse(stored) as Partial<AppSettings>) }
      : defaultSettings(this.platform);
    this.permissions = [
      { kind: "accessibility", state: "not-determined", required: true },
      { kind: "input-monitoring", state: this.platform === "macos" ? "not-determined" : "unavailable", required: false },
      { kind: "microphone", state: "not-determined", required: true },
      { kind: "speech-recognition", state: "not-determined", required: this.platform === "macos" },
    ];
  }

  async getContext(): Promise<AppContext> {
    return {
      platform: this.platform,
      surface: surfaceFromLabel(undefined),
      version: "0.1.0-dev",
      development: true,
      paused: false,
    };
  }

  async getSettings() {
    return structuredClone(this.settings);
  }

  async updateSettings(patch: Partial<AppSettings>) {
    this.settings = { ...this.settings, ...patch };
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
      permission.kind === kind ? { ...permission, state: "granted" } : permission,
    );
    this.emit("permission-status-changed", structuredClone(this.permissions));
    return structuredClone(this.permissions);
  }

  async openPermissionSettings() {}

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

  async getApiKeyStatus() {
    return { ...this.apiKeyStatus };
  }

  async saveApiKey(apiKey: string) {
    await delay(250);
    this.apiKeyStatus = { configured: apiKey.trim().length > 8, connection: "untested" };
    return { ...this.apiKeyStatus };
  }

  async clearApiKey() {
    this.apiKeyStatus = { configured: false, connection: "untested" };
    return { ...this.apiKeyStatus };
  }

  async testApiKey() {
    this.apiKeyStatus = { ...this.apiKeyStatus, connection: "testing" };
    await delay(700);
    this.apiKeyStatus = {
      configured: this.apiKeyStatus.configured,
      connection: this.apiKeyStatus.configured ? "connected" : "invalid",
    };
    return { ...this.apiKeyStatus };
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
    return {
      hasSelection: true,
      applicationName: this.platform === "macos" ? "TextEdit" : "Notepad",
      canReplace: true,
      bounds: { x: 480, y: 320, width: 164, height: 22 },
      initialText: "Hello, how are you?",
    };
  }

  async runWritingAction(request: WritingRequest): Promise<WritingResponse> {
    await delay(850);
    if (request.action === "chat") {
      return { kind: "result", text: `You asked: ${request.text ?? "(nothing)"}` };
    }
    if (request.action === "summarize") {
      return { kind: "result", text: "A short greeting that asks how the other person is doing." };
    }
    if (request.action === "key-points") {
      return { kind: "result", text: "- Opens with a casual greeting\n- Asks how the recipient is doing" };
    }
    return { kind: "replaced" };
  }

  async replaceWritingResult() {}

  async copyText(text: string) {
    await navigator.clipboard?.writeText(text);
  }

  async closeSurface() {}
  async showSurface(surface: Surface) {
    window.location.search = `?surface=${surface}&harness=1`;
  }
  async setSurfaceMode() {}
  async completeOnboarding() {
    await this.updateSettings({ onboardingComplete: true });
  }
  async setPaused() {}
  async checkForUpdates() {
    await delay(450);
    return { currentVersion: "0.1.0-dev", available: false };
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

export { surfaceFromLabel };
