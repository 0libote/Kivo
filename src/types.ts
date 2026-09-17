export type Platform = "macos" | "windows";
export type Surface = "flow-bar" | "writing-tools" | "settings" | "onboarding";
export type ThemePreference = "system" | "light" | "dark";
export type PermissionKind =
  | "accessibility"
  | "input-monitoring"
  | "microphone"
  | "speech-recognition";
export type PermissionState = "not-determined" | "granted" | "denied" | "unavailable";

export interface AppContext {
  platform: Platform;
  surface: Surface;
  version: string;
  development: boolean;
  paused: boolean;
}

export type WritingActionId =
  | "proofread"
  | "rewrite"
  | "friendly"
  | "professional"
  | "concise"
  | "summarize"
  | "key-points"
  | "custom";

export type WritingPopupAnchor = "cursor" | "selection" | "fixed";

/** AI backends (mirrors `AiProvider` in `src-tauri/src/ai/providers.rs`). */
export type AiProviderId = "gemini" | "zen" | "go" | "custom";

export interface AiProviderInfo {
  id: AiProviderId;
  label: string;
  keyUrl: string | null;
  keyOptional: boolean;
  defaultModel: string;
  defaultBaseUrl: string | null;
  supportsLinkSummary: boolean;
  /** Gemini tests with a free metadata check; others send 1 token. */
  testUsesQuota: boolean;
}

export interface AppSettings {
  launchAtLogin: boolean;
  theme: ThemePreference;
  showIdleFlowBar: boolean;
  startInBackground: boolean;
  dictationShortcut: string;
  microphoneId: string | null;
  improveDictationWithAi: boolean;
  dictationLanguage: string;
  soundFeedback: boolean;
  dictationTapEnabled: boolean;
  dictationHoldEnabled: boolean;
  dictationHoldThresholdMs: number;
  writingShortcut: string;
  enabledWritingActions: WritingActionId[];
  writingPopupAnchor: WritingPopupAnchor;
  writingPopupX: number;
  writingPopupY: number;
  writingPopupWidth: number;
  writingPopupHeight: number;
  writingAllowManualText: boolean;
  aiProvider: AiProviderId;
  /** Ordered failover queue: tried top to bottom until one succeeds. */
  aiModels: string[];
  aiCustomBaseUrl: string | null;
  onboardingComplete: boolean;
}

export interface PermissionStatus {
  kind: PermissionKind;
  state: PermissionState;
  required: boolean;
  explanation?: string;
}

export interface MicrophoneDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface SpeechLanguage {
  code: string;
  name: string;
  installed: boolean;
  downloadable: boolean;
}

export interface ApiKeyStatus {
  configured: boolean;
  connection: "untested" | "testing" | "connected" | "invalid" | "rate-limited" | "offline";
}

export interface AiModelInfo {
  id: string;
  label: string;
  description: string;
  /** Short cost summary, e.g. `$0.95 in / $4.00 out per 1M · $60/mo incl.`. */
  cost?: string | null;
  inputPer1M?: number | null;
  outputPer1M?: number | null;
  monthlyLimitUsd?: number | null;
  /** pay_per_token | zen_credits | go_subscription | free | local */
  billing?: string | null;
}

export interface SelectionContext {
  hasSelection: boolean;
  applicationName: string;
  canReplace: boolean;
  bounds?: { x: number; y: number; width: number; height: number };
  /** Captured highlight for the manual text box; empty when nothing is selected. */
  initialText: string;
}

export interface WritingRequest {
  action: WritingActionId;
  instruction?: string;
  /** Edited text-box content, or the explicit summary text / URL. */
  text?: string;
  sourceKind?: "text" | "link";
}

export interface SummarySource {
  kind: "website" | "youtube";
  url: string;
}

export interface WritingResponse {
  kind: "replaced" | "result";
  text?: string;
  source?: SummarySource;
  canReplace?: boolean;
}

export interface DictationSnapshot {
  status: "hidden" | "idle" | "starting" | "listening" | "processing" | "success" | "error";
  sessionId?: string;
  level?: number;
  message?: string;
  canRetry?: boolean;
}

export interface NativeErrorShape {
  code: string;
  message: string;
  recoverable?: boolean;
}

export class NativeError extends Error {
  readonly code: string;
  readonly recoverable: boolean;

  constructor(shape: NativeErrorShape) {
    super(shape.message);
    this.name = "NativeError";
    this.code = shape.code;
    this.recoverable = shape.recoverable ?? false;
  }
}

export const DEFAULT_WRITING_ACTIONS: WritingActionId[] = [
  "proofread",
  "rewrite",
  "friendly",
  "professional",
  "concise",
  "summarize",
  "key-points",
  "custom",
];

export const DEFAULT_AI_MODEL = "gemini-3.8-flash";
export const DEFAULT_AI_PROVIDER: AiProviderId = "gemini";

export function defaultSettings(platform: Platform): AppSettings {
  return {
    launchAtLogin: false,
    theme: "system",
    showIdleFlowBar: false,
    startInBackground: true,
    dictationShortcut: platform === "macos" ? "Fn" : "Ctrl+Meta",
    microphoneId: null,
    improveDictationWithAi: true,
    dictationLanguage: "auto",
    soundFeedback: true,
    dictationTapEnabled: true,
    dictationHoldEnabled: true,
    dictationHoldThresholdMs: 350,
    writingShortcut: platform === "macos" ? "Ctrl+Shift+Space" : "Ctrl+Space",
    enabledWritingActions: [...DEFAULT_WRITING_ACTIONS],
    writingPopupAnchor: "cursor",
    writingPopupX: 480,
    writingPopupY: 320,
    writingPopupWidth: 380,
    writingPopupHeight: 460,
    writingAllowManualText: true,
    aiProvider: DEFAULT_AI_PROVIDER,
    aiModels: [DEFAULT_AI_MODEL],
    aiCustomBaseUrl: null,
    onboardingComplete: false,
  };
}
