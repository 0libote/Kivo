import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  type AiModelInfo,
  type AiProviderId,
  type AiProviderInfo,
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  type DictationSnapshot,
  type LocalAiServerInfo,
  type LocalModelProgress,
  type LocalSpeechModelInfo,
  type MicrophoneDevice,
  type NativeErrorShape,
  type PermissionKind,
  type PermissionStatus,
  type Platform,
  type SelectionContext,
  type SpeechLanguage,
  type Surface,
  type VozModelProgress,
  type VozModelStatus,
  type VozWorkerReply,
  type VozWorkerRequest,
  type WritingRequest,
  type WritingResponse,
} from "../types";
export type NativeEventMap = {
  "show-about": null;
  "recovery-changed": null;
  "pause-changed": boolean;
  "dictation-state": DictationSnapshot;
  "dictation-level": number;
  "writing-context": SelectionContext;
  "writing-error": NativeErrorShape;
  "permission-status-changed": PermissionStatus[];
  "settings-changed": AppSettings;
  "local-models-changed": LocalSpeechModelInfo[];
  "local-model-progress": LocalModelProgress;
  "voz-model-status": VozModelProgress;
  "voz-worker-request": VozWorkerRequest;
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
  frontendReady(): Promise<void>;
  getContext(): Promise<AppContext>;
  getSettings(): Promise<AppSettings>;
  getDictationRecovery(): Promise<string | null>;
  clearDictationRecovery(): Promise<void>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  getPermissions(): Promise<PermissionStatus[]>;
  requestPermission(kind: PermissionKind): Promise<PermissionStatus[]>;
  openPermissionSettings(kind: PermissionKind): Promise<void>;
  listMicrophones(): Promise<MicrophoneDevice[]>;
  listSpeechLanguages(): Promise<SpeechLanguage[]>;
  listLocalSpeechModels(): Promise<LocalSpeechModelInfo[]>;
  downloadLocalSpeechModel(modelId: string): Promise<LocalSpeechModelInfo[]>;
  cancelLocalSpeechModelDownload(modelId: string): Promise<void>;
  deleteLocalSpeechModel(modelId: string): Promise<LocalSpeechModelInfo[]>;
  getVozModelStatus(): Promise<VozModelStatus>;
  downloadVozModel(): Promise<void>;
  deleteVozModel(): Promise<void>;
  completeVozWorkerRequest(reply: VozWorkerReply): Promise<void>;
  setVozWorkerReady(ready: boolean): Promise<void>;
  listAiProviders(): Promise<AiProviderInfo[]>;
  listAiModels(provider?: AiProviderId): Promise<AiModelInfo[]>;
  detectLocalAiServers(): Promise<LocalAiServerInfo[]>;
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
  setSurfaceMode(
    surface: "writing-tools",
    mode: "menu" | "custom" | "summary" | "processing" | "result" | "error",
    height?: number,
  ): Promise<void>;
  completeOnboarding(): Promise<void>;
  setPaused(paused: boolean): Promise<void>;
  checkForUpdates(): Promise<UpdateResult>;
  installUpdate(): Promise<void>;
  restartApp(): Promise<void>;
  openExternal(url: string): Promise<void>;
  on<K extends keyof NativeEventMap>(
    event: K,
    handler: (payload: NativeEventMap[K]) => void,
  ): Promise<UnlistenFn>;
}

export function detectedPlatform(): Platform {
  if (/Windows/i.test(navigator.userAgent)) return "windows";
  if (/Linux|X11/i.test(navigator.userAgent)) return "linux";
  return "windows";
}

export function surfaceFromLabel(label: string | undefined): Surface {
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
