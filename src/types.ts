import type { IconName } from "./components/Icon";

/**
 * Every OS runs one shared AppCore; only a thin adapter differs per host.
 * `linux` is the dev/test bench (simulated speech + file vault) so the full
 * app is exercisable on Linux even though only macOS/Windows ship. If a flow
 * works on one platform it works on all three unless the adapter says
 * otherwise — and the adapter surface is ~6 methods, all unit-tested.
 */
export type Platform = "macos" | "windows" | "linux";
/**
 * Native windows plus `gallery`: a frontend-only dev bench (never a Tauri
 * window, never an IPC surface) that renders every bit inline for Linux
 * verification and screenshots.
 */
export type Surface = "flow-bar" | "writing-tools" | "settings" | "onboarding" | "gallery";
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

/**
 * A user-editable Writing Tools preset. Built-in presets keep their stable id
 * (`proofread`, …); custom presets use a generated `custom-…` id and always
 * run through the native `custom` action with an explicit system prompt.
 *
 * `template` is the advanced override of the full system prompt. `null` uses
 * the default template with `{{instruction}}` / `{{outputRule}}` placeholders,
 * so the easy "what should it do?" field keeps working even after edits.
 */
export interface WritingPreset {
  id: string;
  label: string;
  description: string;
  icon: IconName;
  /** The task instruction — the easy "what it should do" field. */
  instruction: string;
  /** Advanced: the full system-prompt template, or null for the default. */
  template: string | null;
  /** Whether the result replaces the selection (false shows an informational result). */
  replacesSelection: boolean;
  /** Per-preset model priority; empty follows the global AI model queue. */
  models: string[];
}

/** AI backends (mirrors `AiProvider` in `src-tauri/src/ai/providers.rs`). */
export type AiProviderId = "gemini" | "zen" | "go" | "custom";

/** Which recognizer powers dictation (mirrors `SpeechEnginePreference`). */
export type SpeechEngineId = "system" | "local";

/** One downloadable on-device speech model. */
export interface LocalSpeechModelInfo {
  id: string;
  name: string;
  description: string;
  sizeBytes: number;
  recommended: boolean;
  downloaded: boolean;
  /** 0–100 quality score (higher is more accurate), for the comparison bars. */
  accuracy: number;
  /** 0–100 speed score (higher is faster), for the comparison bars. */
  speed: number;
  /** Model family, e.g. "Whisper" or "Parakeet". */
  family: string;
  /** Parameter count label, e.g. "0.6B". */
  parameters: string;
  /** Number of languages the model can transcribe. */
  languageCount: number;
  /** Whether the model can stream partial results. */
  streaming: boolean;
}

export interface LocalModelProgress {
  modelId: string;
  downloaded: number;
  total: number;
}

/** A locally running OpenAI-compatible server Kivo can use for local LLMs. */
export interface LocalAiServerInfo {
  id: string;
  name: string;
  baseUrl: string;
  running: boolean;
  models: string[];
}

export interface LocalAiInstallProgress {
  downloaded: number;
  total: number;
}

export interface AiProviderInfo {
  id: AiProviderId;
  label: string;
  keyUrl: string | null;
  keyOptional: boolean;
  defaultModel: string;
  defaultBaseUrl: string | null;
  supportsLinkSummary: boolean;
  /** Connection tests send a short generation request and consume quota. */
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
  speechEngine: SpeechEngineId;
  /** On-device model id; null means the recommended model. */
  localSpeechModel: string | null;
  /** Model used to clean up dictated text; null follows the Writing Tools queue. */
  dictationCleanupModel: string | null;
  writingShortcut: string;
  /** Ordered ids of the enabled Writing Tools presets (built-in or custom). */
  enabledWritingActions: string[];
  /** Overridden built-ins and custom presets. Untouched built-ins use defaults. */
  writingPresets: WritingPreset[];
  aiProvider: AiProviderId;
  /** Ordered failover queue: tried top to bottom until one succeeds. */
  aiModels: string[];
  /** Fast/balanced/deep preset; unsupported model/provider controls are omitted. */
  aiReasoningMode: "fast" | "balanced" | "deep";
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
  connection:
    | "untested"
    | "testing"
    | "connected"
    | "invalid"
    | "rate-limited"
    | "offline"
    | "model"
    | "blocked";
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
  /** Captured highlight; empty when nothing is selected. */
  initialText: string;
}

export interface WritingRequest {
  /** Built-in action id; custom presets send `custom`. */
  action: WritingActionId;
  /** The preset that triggered the request (for the enabled check). */
  presetId?: string;
  instruction?: string;
  /** Fully resolved system prompt for editable presets. */
  systemInstruction?: string;
  /** Overrides the built-in replace/show-result behavior. */
  replacesSelection?: boolean;
  /** Per-preset model priority; empty/omitted follows the global queue. */
  models?: string[];
  /** Captured selection sent to the requested action. */
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

/**
 * Per-platform defaults. Mirrors `dictation_default_for` /
 * `writing_tools_default_for` in `src-tauri/src/config/mod.rs` (checked by
 * `scripts/check-platform-parity.ts`). Linux uses the portable
 * Control+Alt+Space dictation hold (no native Fn / Ctrl+Win monitor there)
 * and shares the Windows writing shortcut so writing behavior matches the
 * bench. Kept as data, not branches, so adding a platform is a compiler
 * error until its defaults are chosen.
 */
const DICTATION_SHORTCUTS: Record<Platform, string> = {
  macos: "Fn",
  windows: "Ctrl+Meta",
  linux: "Control+Alt+Space",
};

const WRITING_SHORTCUTS: Record<Platform, string> = {
  macos: "Ctrl+Shift+Space",
  windows: "Ctrl+Space",
  linux: "Ctrl+Space",
};

export function defaultSettings(platform: Platform): AppSettings {
  const dictationShortcut = DICTATION_SHORTCUTS[platform];
  const writingShortcut = WRITING_SHORTCUTS[platform];
  return {
    launchAtLogin: false,
    theme: "system",
    showIdleFlowBar: false,
    startInBackground: true,
    dictationShortcut,
    microphoneId: null,
    improveDictationWithAi: true,
    dictationLanguage: "auto",
    soundFeedback: true,
    dictationTapEnabled: true,
    dictationHoldEnabled: true,
    dictationHoldThresholdMs: 350,
    speechEngine: "system",
    localSpeechModel: null,
    dictationCleanupModel: null,
    writingShortcut,
    enabledWritingActions: [...DEFAULT_WRITING_ACTIONS],
    writingPresets: [],
    aiProvider: DEFAULT_AI_PROVIDER,
    aiModels: [DEFAULT_AI_MODEL],
    aiReasoningMode: "fast",
    aiCustomBaseUrl: null,
    onboardingComplete: false,
  };
}
