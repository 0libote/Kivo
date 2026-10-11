import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  type AiModelInfo,
  type AiProviderId,
  type AiProviderInfo,
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  type LocalAiServerInfo,
  type LocalSpeechModelInfo,
  type MicrophoneDevice,
  NativeError,
  type NativeErrorShape,
  type PermissionKind,
  type PermissionStatus,
  type SelectionContext,
  type SpeechLanguage,
  type Surface,
  type VozModelStatus,
  type VozWorkerReply,
  type WritingRequest,
  type WritingResponse,
} from "../types";
import {
  detectedPlatform,
  type NativeBridge,
  type NativeEventMap,
  surfaceFromLabel,
  type UpdateResult,
} from "./bridge";

function normalizeError(error: unknown): NativeError {
  if (error instanceof NativeError) return error;
  if (typeof error === "object" && error !== null && "message" in error) {
    const shape = error as Partial<NativeErrorShape>;
    return new NativeError({
      code: shape.code ?? "native-error",
      message:
        typeof shape.message === "string" ? shape.message : "The operation couldn’t be completed.",
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
  frontendReady = () => call<void>("frontend_ready");

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
  requestPermission = (kind: PermissionKind) =>
    call<PermissionStatus[]>("request_permission", { kind });
  openPermissionSettings = (kind: PermissionKind) =>
    call<void>("open_permission_settings", { kind });
  listMicrophones = () => call<MicrophoneDevice[]>("list_microphones");
  listSpeechLanguages = () => call<SpeechLanguage[]>("list_speech_languages");
  listLocalSpeechModels = () => call<LocalSpeechModelInfo[]>("list_local_speech_models");
  downloadLocalSpeechModel = (modelId: string) =>
    call<LocalSpeechModelInfo[]>("download_local_speech_model", { modelId });
  cancelLocalSpeechModelDownload = (modelId: string) =>
    call<void>("cancel_local_speech_model_download", { modelId });
  deleteLocalSpeechModel = (modelId: string) =>
    call<LocalSpeechModelInfo[]>("delete_local_speech_model", { modelId });
  getVozModelStatus = () => call<VozModelStatus>("get_voz_model_status");
  downloadVozModel = () => call<void>("download_voz_model");
  deleteVozModel = () => call<void>("delete_voz_model");
  completeVozWorkerRequest = (reply: VozWorkerReply) =>
    call<void>("complete_voz_worker_request", { reply });
  setVozWorkerReady = (ready: boolean) => call<void>("set_voz_worker_ready", { ready });
  listAiProviders = () => call<AiProviderInfo[]>("list_ai_providers");
  listAiModels = (provider?: AiProviderId) =>
    call<AiModelInfo[]>("list_ai_models", { provider: provider ?? null });
  detectLocalAiServers = () => call<LocalAiServerInfo[]>("detect_local_ai_servers");
  getApiKeyStatus = () => call<ApiKeyStatus>("get_api_key_status");
  saveApiKey = (apiKey: string) => call<ApiKeyStatus>("store_api_key", { apiKey });
  clearApiKey = () => call<ApiKeyStatus>("remove_api_key");
  testApiKey = () => call<ApiKeyStatus>("test_api_key");
  startDictation = () => call<void>("start_dictation");
  stopDictation = () => call<void>("stop_dictation");
  cancelDictation = () => call<void>("cancel_dictation");
  retryDictation = () => call<void>("retry_dictation");
  getWritingContext = () => call<SelectionContext>("get_writing_context");
  runWritingAction = (request: WritingRequest) =>
    call<WritingResponse>("run_writing_action", { request });
  replaceWritingResult = (text: string) => call<void>("replace_writing_result", { text });
  copyText = (text: string) => call<void>("copy_text", { text });
  closeSurface = (surface: Surface) => call<void>("close_surface", { surface });
  showSurface = (surface: Surface) => call<void>("show_surface", { surface });
  setSurfaceMode = (
    surface: "writing-tools",
    mode: "menu" | "custom" | "summary" | "processing" | "result" | "error",
    height?: number,
  ) => call<void>("set_surface_mode", { surface, mode, height });
  completeOnboarding = () => call<void>("complete_onboarding");
  setPaused = (paused: boolean) => call<void>("set_paused", { paused });
  checkForUpdates = () => call<UpdateResult>("check_for_updates");
  installUpdate = () => call<void>("install_update");
  restartApp = () => call<void>("restart_app");
  openExternal = (url: string) => call<void>("open_external", { url });

  on<K extends keyof NativeEventMap>(
    event: K,
    handler: (payload: NativeEventMap[K]) => void,
  ): Promise<UnlistenFn> {
    return listen<NativeEventMap[K]>(event, ({ payload }) => handler(payload));
  }
}

// Production desktop output excludes the complete simulation graph. Explicit
// browser builds keep it for previews and production frontend smoke tests.
const browserHarness = import.meta.env?.PROD !== true || import.meta.env.VITE_KIVO_HARNESS === "1";
export const nativeBridge: NativeBridge = window.__TAURI_INTERNALS__
  ? new TauriBridge()
  : browserHarness
    ? new (await import("./mock")).MockBridge()
    : (() => {
        throw new Error("Kivo's desktop bridge is unavailable.");
      })();

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

export type { NativeBridge, UpdateResult } from "./bridge";
export { detectedPlatform, surfaceFromLabel } from "./bridge";
