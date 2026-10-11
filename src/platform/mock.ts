import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  fallbackAiModels,
  isUsableAiModelIdFor,
  migrateAiModels,
  normalizeAiModelList,
  normalizeAiProvider,
} from "../ai/models";
import { FALLBACK_AI_PROVIDERS } from "../ai/providers";
import { normalizeVocabularySetting } from "../features/settings/vocabulary";
import {
  type AiModelInfo,
  type AiProviderId,
  type AiProviderInfo,
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  defaultSettings,
  type LocalAiServerInfo,
  type LocalSpeechModelInfo,
  type MicrophoneDevice,
  type PermissionKind,
  type PermissionStatus,
  type Platform,
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
} from "./bridge";

type UntypedListener = (payload: unknown) => void;
type ListenerMap = Partial<Record<keyof NativeEventMap, Set<UntypedListener>>>;

export class MockBridge implements NativeBridge {
  readonly isNative = false;
  frontendReady = () => Promise.resolve();
  private readonly platform = detectedPlatform();
  private settings: AppSettings;
  private vozStatus: VozModelStatus = {
    supported: true,
    downloaded: false,
    phase: "notDownloaded",
    progress: null,
    runtime: "Simulated test engine",
    error: null,
  };
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
    // Early harness settings stored bare word lists; coerce them to entries.
    this.settings.dictationVocabulary = normalizeVocabularySetting(
      this.settings.dictationVocabulary,
    );
    // Windows has no in-app consent prompt; the Linux bench simulates speech.
    this.permissions = [
      { kind: "accessibility", state: "not-determined", required: true },
      {
        kind: "input-monitoring",
        state: "unavailable",
        required: false,
      },
      { kind: "microphone", state: "granted", required: true },
      {
        kind: "speech-recognition",
        state: "granted",
        required: false,
      },
    ];
  }

  getContext(): Promise<AppContext> {
    return Promise.resolve({
      platform: this.platform,
      surface: surfaceFromLabel(undefined),
      version: "0.1.0-dev",
      development: true,
      paused: this.paused,
    });
  }

  getDictationRecovery(): Promise<string | null> {
    return Promise.resolve(null);
  }
  clearDictationRecovery(): Promise<void> {
    this.emit("recovery-changed", null);
    return Promise.resolve();
  }

  getSettings(): Promise<AppSettings> {
    return Promise.resolve(structuredClone(this.settings));
  }

  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
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
    if (patch.dictationVocabulary !== undefined) {
      this.settings.dictationVocabulary = normalizeVocabularySetting(
        this.settings.dictationVocabulary,
      );
    }
    // Mirror the native normalization: a cleanup override that no longer fits
    // the selected provider is dropped so cleanup falls back to the queue.
    if (
      this.settings.dictationCleanupModel &&
      !isUsableAiModelIdFor(this.settings.aiProvider, this.settings.dictationCleanupModel)
    ) {
      this.settings.dictationCleanupModel = null;
    }
    window.localStorage.setItem("kivo-dev-settings", JSON.stringify(this.settings));
    this.emit("settings-changed", structuredClone(this.settings));
    return Promise.resolve(structuredClone(this.settings));
  }

  getPermissions(): Promise<PermissionStatus[]> {
    return Promise.resolve(structuredClone(this.permissions));
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

  openPermissionSettings(): Promise<void> {
    // Intentional no-op: browser harness has no OS settings screen to open.
    return Promise.resolve();
  }

  listMicrophones(): Promise<MicrophoneDevice[]> {
    return Promise.resolve([
      { id: "default", name: "System Default", isDefault: true },
      { id: "studio-display", name: "Studio Display Microphone", isDefault: false },
    ]);
  }

  listSpeechLanguages(): Promise<SpeechLanguage[]> {
    return Promise.resolve([
      { code: "auto", name: "Automatic", installed: true, downloadable: false },
      { code: "en-GB", name: "English (United Kingdom)", installed: true, downloadable: false },
      { code: "en-US", name: "English (United States)", installed: true, downloadable: false },
      { code: "fr-FR", name: "French (France)", installed: false, downloadable: true },
    ]);
  }

  // Illustrative catalog mirroring src-tauri/src/speech/model_store.rs; the
  // harness never downloads anything.
  private readonly localModels: LocalSpeechModelInfo[] = [
    {
      id: "whisper-tiny",
      name: "Whisper Tiny",
      description: "Fastest and smallest.",
      sizeBytes: 44_211_616,
      recommended: false,
      downloaded: false,
      accuracy: 61,
      speed: 100,
      family: "Whisper",
      parameters: "38M",
      languageCount: 99,
      streaming: false,
    },
    {
      id: "whisper-base",
      name: "Whisper Base",
      description: "A light, responsive model.",
      sizeBytes: 63_786_048,
      recommended: false,
      downloaded: false,
      accuracy: 71,
      speed: 99,
      family: "Whisper",
      parameters: "73M",
      languageCount: 99,
      streaming: false,
    },
    {
      id: "whisper-small",
      name: "Whisper Small",
      description: "Recommended.",
      sizeBytes: 193_749_056,
      recommended: true,
      downloaded: true,
      accuracy: 80,
      speed: 78,
      family: "Whisper",
      parameters: "242M",
      languageCount: 99,
      streaming: false,
    },
    {
      id: "whisper-medium",
      name: "Whisper Medium",
      description: "Higher accuracy.",
      sizeBytes: 504_102_848,
      recommended: false,
      downloaded: false,
      accuracy: 84,
      speed: 42,
      family: "Whisper",
      parameters: "764M",
      languageCount: 99,
      streaming: false,
    },
    {
      id: "whisper-large-v3-turbo",
      name: "Whisper Large v3 Turbo",
      description: "Best quality.",
      sizeBytes: 536_069_728,
      recommended: false,
      downloaded: false,
      accuracy: 88,
      speed: 35,
      family: "Whisper",
      parameters: "809M",
      languageCount: 100,
      streaming: false,
    },
    {
      id: "parakeet-tdt-0.6b-v3",
      name: "Parakeet TDT 0.6B v3",
      description: "Fast across 25 European languages.",
      sizeBytes: 739_508_576,
      recommended: false,
      downloaded: false,
      accuracy: 88,
      speed: 79,
      family: "Parakeet",
      parameters: "0.6B",
      languageCount: 25,
      streaming: false,
    },
    {
      id: "canary-180m-flash",
      name: "Canary 180M Flash",
      description: "Tiny and instant.",
      sizeBytes: 218_447_552,
      recommended: false,
      downloaded: false,
      accuracy: 88,
      speed: 98,
      family: "Canary",
      parameters: "180M",
      languageCount: 4,
      streaming: false,
    },
    {
      id: "moonshine-base",
      name: "Moonshine Base",
      description: "Small English model.",
      sizeBytes: 77_476_480,
      recommended: false,
      downloaded: false,
      accuracy: 80,
      speed: 99,
      family: "Moonshine",
      parameters: "62M",
      languageCount: 1,
      streaming: false,
    },
    {
      id: "sensevoice-small",
      name: "SenseVoice Small",
      description: "Strong on Asian languages.",
      sizeBytes: 252_684_608,
      recommended: false,
      downloaded: false,
      accuracy: 81,
      speed: 98,
      family: "SenseVoice",
      parameters: "234M",
      languageCount: 5,
      streaming: false,
    },
    {
      id: "cohere-transcribe-03-2026",
      name: "Cohere Transcribe",
      description: "Highest accuracy, slower.",
      sizeBytes: 1_770_270_208,
      recommended: false,
      downloaded: false,
      accuracy: 92,
      speed: 63,
      family: "Cohere",
      parameters: "2.0B",
      languageCount: 14,
      streaming: false,
    },
  ];

  listLocalSpeechModels(): Promise<LocalSpeechModelInfo[]> {
    return Promise.resolve(structuredClone(this.localModels));
  }

  async downloadLocalSpeechModel(modelId: string) {
    const model = this.localModels.find((candidate) => candidate.id === modelId);
    if (model && !model.downloaded) {
      await [1, 2, 3, 4].reduce(
        (previous, step) =>
          previous.then(() => {
            this.emit("local-model-progress", {
              modelId,
              downloaded: Math.round((model.sizeBytes * step) / 4),
              total: model.sizeBytes,
            });
            return delay(180);
          }),
        Promise.resolve(),
      );
      model.downloaded = true;
    }
    this.emit("local-models-changed", structuredClone(this.localModels));
    return structuredClone(this.localModels);
  }

  cancelLocalSpeechModelDownload(): Promise<void> {
    // Intentional no-op: the harness download resolves immediately.
    return Promise.resolve();
  }

  deleteLocalSpeechModel(modelId: string): Promise<LocalSpeechModelInfo[]> {
    const model = this.localModels.find((candidate) => candidate.id === modelId);
    if (model) model.downloaded = false;
    this.emit("local-models-changed", structuredClone(this.localModels));
    return Promise.resolve(structuredClone(this.localModels));
  }

  getVozModelStatus(): Promise<VozModelStatus> {
    return Promise.resolve(structuredClone(this.vozStatus));
  }

  async downloadVozModel() {
    if (!this.vozStatus.supported || this.vozStatus.downloaded) return;
    this.vozStatus = { ...this.vozStatus, phase: "downloading", progress: 0 };
    await [0.2, 0.45, 0.7, 1].reduce(
      (previous, progress) =>
        previous.then(() => {
          this.vozStatus = { ...this.vozStatus, progress };
          this.emit("voz-model-status", {
            phase: "downloading",
            progress,
            error: null,
          });
          // Keep each simulated progress state visible to the browser harness.
          return delay(120);
        }),
      Promise.resolve(),
    );
    this.vozStatus = { ...this.vozStatus, phase: "preparing", progress: null };
    this.emit("voz-model-status", { phase: "preparing", progress: null, error: null });
    await delay(240);
    this.vozStatus = { ...this.vozStatus, downloaded: true, phase: "ready", progress: null };
    this.emit("voz-model-status", { phase: "ready", progress: null, error: null });
  }

  deleteVozModel(): Promise<void> {
    this.vozStatus = {
      ...this.vozStatus,
      downloaded: false,
      phase: "notDownloaded",
      progress: null,
    };
    this.emit("voz-model-status", { phase: "notDownloaded", progress: null, error: null });
    return Promise.resolve();
  }

  completeVozWorkerRequest(_reply: VozWorkerReply): Promise<void> {
    // The browser harness simulates the runtime directly; it never starts a
    // model worker or downloads Voz assets.
    return Promise.resolve();
  }

  setVozWorkerReady(_ready: boolean): Promise<void> {
    return Promise.resolve();
  }

  listAiProviders(): Promise<AiProviderInfo[]> {
    return Promise.resolve(structuredClone(FALLBACK_AI_PROVIDERS));
  }

  listAiModels(provider?: AiProviderId): Promise<AiModelInfo[]> {
    return Promise.resolve(structuredClone(fallbackAiModels(provider ?? this.settings.aiProvider)));
  }

  detectLocalAiServers(): Promise<LocalAiServerInfo[]> {
    return Promise.resolve([
      {
        id: "ollama",
        name: "Ollama",
        baseUrl: "http://localhost:11434/v1",
        running: false,
        models: [],
      },
      {
        id: "lmstudio",
        name: "LM Studio",
        baseUrl: "http://localhost:1234/v1",
        running: false,
        models: [],
      },
      {
        id: "llamacpp",
        name: "llama.cpp",
        baseUrl: "http://localhost:8080/v1",
        running: false,
        models: [],
      },
    ]);
  }

  private currentKeyStatus(): ApiKeyStatus {
    return { ...this.apiKeyStatuses[this.settings.aiProvider] };
  }

  getApiKeyStatus(): Promise<ApiKeyStatus> {
    return Promise.resolve(this.currentKeyStatus());
  }

  async saveApiKey(apiKey: string) {
    await delay(250);
    const provider = this.settings.aiProvider;
    this.apiKeyStatuses[provider] = {
      configured: apiKey.trim().length > 8,
      connection: "untested",
    };
    return this.currentKeyStatus();
  }

  clearApiKey(): Promise<ApiKeyStatus> {
    this.apiKeyStatuses[this.settings.aiProvider] = { configured: false, connection: "untested" };
    return Promise.resolve(this.currentKeyStatus());
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

  startDictation(): Promise<void> {
    this.emit("dictation-state", { status: "listening", sessionId: crypto.randomUUID() });
    return Promise.resolve();
  }

  async stopDictation() {
    this.emit("dictation-state", { status: "processing" });
    await delay(700);
    this.emit("dictation-state", { status: "success" });
  }

  cancelDictation(): Promise<void> {
    this.emit("dictation-state", { status: "hidden" });
    return Promise.resolve();
  }

  retryDictation(): Promise<void> {
    return this.startDictation();
  }

  getWritingContext(): Promise<SelectionContext> {
    const applicationNames: Record<Platform, string> = {
      windows: "Notepad",
      linux: "Text Editor",
    };
    return Promise.resolve({
      hasSelection: true,
      applicationName: applicationNames[this.platform],
      canReplace: true,
      bounds: { x: 480, y: 320, width: 164, height: 22 },
      initialText: "Hello, how are you?",
    });
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
      return {
        kind: "result",
        text: "A short greeting that asks how the other person is doing.",
        canReplace: true,
      };
    }
    if (request.action === "key-points") {
      return {
        kind: "result",
        text: "- Opens with a casual greeting\n- Asks how the recipient is doing",
      };
    }
    // Presets that show a result instead of replacing the selection.
    if (request.replacesSelection === false) {
      return { kind: "result", text: "A preview result for this preset." };
    }
    return { kind: "replaced" };
  }

  replaceWritingResult(): Promise<void> {
    // Intentional no-op: browser harness previews results instead of replacing text.
    return Promise.resolve();
  }

  async copyText(text: string) {
    await navigator.clipboard?.writeText(text);
  }

  closeSurface(): Promise<void> {
    // Intentional no-op: browser harness keeps surfaces visible for development.
    return Promise.resolve();
  }
  showSurface(surface: Surface): Promise<void> {
    window.location.search = `?surface=${surface}&harness=1`;
    return Promise.resolve();
  }
  setSurfaceMode(): Promise<void> {
    // Intentional no-op: browser harness does not resize native windows.
    return Promise.resolve();
  }
  completeOnboarding(): Promise<void> {
    return this.updateSettings({ onboardingComplete: true }).then(() => undefined);
  }
  setPaused(paused: boolean): Promise<void> {
    this.paused = paused;
    this.emit("pause-changed", paused);
    return Promise.resolve();
  }
  async checkForUpdates() {
    await delay(450);
    return { currentVersion: "0.1.0-dev", available: false };
  }
  async installUpdate() {
    await delay(800);
  }
  restartApp(): Promise<void> {
    // Intentional no-op: browser harness keeps running for development.
    return Promise.resolve();
  }
  openExternal(url: string): Promise<void> {
    window.open(url, "_blank", "noopener,noreferrer");
    return Promise.resolve();
  }

  on<K extends keyof NativeEventMap>(
    event: K,
    handler: (payload: NativeEventMap[K]) => void,
  ): Promise<UnlistenFn> {
    const listeners = (this.listeners[event] ??= new Set());
    const listener: UntypedListener = (payload) => handler(payload as NativeEventMap[K]);
    listeners.add(listener);
    return Promise.resolve(() => listeners.delete(listener));
  }

  private emit<K extends keyof NativeEventMap>(event: K, payload: NativeEventMap[K]) {
    this.listeners[event]?.forEach((listener) => listener(payload));
  }
}

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));
}
