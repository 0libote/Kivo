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
  | "custom"
  | "chat";

export type WritingPopupAnchor = "cursor" | "selection" | "fixed";

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
  writingShortcut: string;
  enabledWritingActions: WritingActionId[];
  writingPopupAnchor: WritingPopupAnchor;
  writingPopupX: number;
  writingPopupY: number;
  writingPopupWidth: number;
  writingPopupHeight: number;
  writingAllowManualText: boolean;
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

export interface SelectionContext {
  hasSelection: boolean;
  applicationName: string;
  canReplace: boolean;
  bounds?: { x: number; y: number; width: number; height: number };
  /** Captured highlight for the manual text box; empty for quick chat. */
  initialText: string;
}

export interface WritingRequest {
  action: WritingActionId;
  instruction?: string;
  /** Edited text-box content, or the quick-chat message. */
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
  status: "hidden" | "idle" | "listening" | "processing" | "success" | "error";
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
    writingShortcut: platform === "macos" ? "Ctrl+Shift+Space" : "Ctrl+Space",
    enabledWritingActions: [...DEFAULT_WRITING_ACTIONS],
    writingPopupAnchor: "cursor",
    writingPopupX: 480,
    writingPopupY: 320,
    writingPopupWidth: 380,
    writingPopupHeight: 460,
    writingAllowManualText: true,
    onboardingComplete: false,
  };
}
