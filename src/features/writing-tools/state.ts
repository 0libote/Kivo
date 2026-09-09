import type { SelectionContext, SummarySource, WritingActionId } from "../../types";

export type WritingMode = "closed" | "menu" | "chat" | "custom" | "summary" | "processing" | "result" | "error";

export interface WritingToolsState {
  mode: WritingMode;
  context: SelectionContext | null;
  enabledActions: WritingActionId[];
  selectedIndex: number;
  activeAction: WritingActionId | null;
  customInstruction: string;
  /** Editable text-box content: captured highlight, or the chat message. */
  sourceText: string;
  summaryKind: "text" | "link";
  summaryInput: string;
  usesSummaryInput: boolean;
  resultSource?: SummarySource;
  resultCanReplace: boolean;
  resultText: string;
  error: string | null;
  canRetry: boolean;
}

export type WritingToolsEvent =
  | { type: "OPEN"; context: SelectionContext; enabledActions: WritingActionId[] }
  | { type: "MOVE"; delta: number }
  | { type: "SELECT"; index: number }
  | { type: "OPEN_CUSTOM"; initialValue?: string }
  | { type: "OPEN_SUMMARY"; kind: "text" | "link" }
  | { type: "SET_SUMMARY_INPUT"; value: string }
  | { type: "SET_CUSTOM"; value: string }
  | { type: "SET_SOURCE"; value: string }
  | { type: "RUN"; action: WritingActionId }
  | { type: "RESULT"; text: string; source?: SummarySource; canReplace?: boolean }
  | { type: "REPLACED" }
  | { type: "FAIL"; message: string; canRetry?: boolean }
  | { type: "BACK" }
  | { type: "CLOSE" };

export const initialWritingToolsState: WritingToolsState = {
  mode: "closed",
  context: null,
  enabledActions: [],
  selectedIndex: 0,
  activeAction: null,
  customInstruction: "",
  sourceText: "",
  summaryKind: "text",
  summaryInput: "",
  usesSummaryInput: false,
  resultCanReplace: false,
  resultText: "",
  error: null,
  canRetry: false,
};

function openState(event: Extract<WritingToolsEvent, { type: "OPEN" }>): WritingToolsState {
  return {
    ...initialWritingToolsState,
    mode: event.context.hasSelection ? "menu" : "chat",
    context: event.context,
    enabledActions: event.enabledActions,
    sourceText: event.context.initialText ?? "",
  };
}

function moveState(state: WritingToolsState, delta: number): WritingToolsState {
  if (state.mode !== "menu" || state.enabledActions.length === 0) return state;
  const length = state.enabledActions.length;
  return { ...state, selectedIndex: (state.selectedIndex + delta + length) % length };
}

function selectState(state: WritingToolsState, index: number): WritingToolsState {
  if (state.mode !== "menu") return state;
  const maxIndex = Math.max(0, state.enabledActions.length - 1);
  return { ...state, selectedIndex: Math.min(Math.max(0, index), maxIndex) };
}

function runState(state: WritingToolsState, action: WritingActionId): WritingToolsState {
  if (!["menu", "custom", "chat", "summary", "error"].includes(state.mode)) return state;
  if ((state.mode === "menu" || state.mode === "custom") && !state.context) return state;
  return { ...state, mode: "processing", activeAction: action, error: null, canRetry: false };
}

function backState(state: WritingToolsState): WritingToolsState {
  if (state.mode === "error" && state.usesSummaryInput) {
    return { ...state, mode: "summary", error: null, canRetry: false };
  }
  if (!["custom", "summary", "error", "result"].includes(state.mode)) return state;
  return {
    ...state,
    mode: state.context?.hasSelection ? "menu" : "chat",
    activeAction: null,
    usesSummaryInput: false,
    resultSource: undefined,
    resultCanReplace: false,
    error: null,
    canRetry: false,
    resultText: "",
  };
}

function setSourceState(state: WritingToolsState, value: string): WritingToolsState {
  if (state.mode !== "menu" && state.mode !== "chat") return state;
  return { ...state, sourceText: value };
}

function summaryInputFor(state: WritingToolsState, kind: "text" | "link"): string {
  if (kind === "text") return state.sourceText;
  const candidate = state.sourceText.trim();
  if (/\s/u.test(candidate)) return "";
  try {
    const url = new URL(candidate);
    return url.protocol === "https:" || url.protocol === "http:" ? candidate : "";
  } catch {
    return "";
  }
}

function openSummaryState(state: WritingToolsState, kind: "text" | "link"): WritingToolsState {
  if (!state.enabledActions.includes("summarize")) return state;
  return {
    ...state,
    mode: "summary",
    activeAction: "summarize",
    summaryKind: kind,
    summaryInput: summaryInputFor(state, kind),
    usesSummaryInput: true,
    error: null,
  };
}

function resultState(state: WritingToolsState, event: Extract<WritingToolsEvent, { type: "RESULT" }>): WritingToolsState {
  if (state.mode !== "processing") return state;
  const isLink = state.usesSummaryInput && state.summaryKind === "link";
  return {
    ...state,
    mode: "result",
    resultText: event.text,
    resultSource: event.source,
    resultCanReplace: !isLink && (event.canReplace ?? state.context?.canReplace ?? false),
    error: null,
  };
}

export function writingToolsReducer(state: WritingToolsState, event: WritingToolsEvent): WritingToolsState {
  switch (event.type) {
    case "OPEN":
      return openState(event);
    case "MOVE":
      return moveState(state, event.delta);
    case "SELECT":
      return selectState(state, event.index);
    case "OPEN_CUSTOM":
      return {
        ...state,
        mode: "custom",
        activeAction: "custom",
        customInstruction: event.initialValue ?? state.customInstruction,
        error: null,
      };
    case "OPEN_SUMMARY":
      return openSummaryState(state, event.kind);
    case "SET_SUMMARY_INPUT":
      return state.mode === "summary" ? { ...state, summaryInput: event.value } : state;
    case "SET_CUSTOM":
      return state.mode === "custom" ? { ...state, customInstruction: event.value } : state;
    case "SET_SOURCE":
      return setSourceState(state, event.value);
    case "RUN":
      return runState(state, event.action);
    case "RESULT":
      return resultState(state, event);
    case "REPLACED":
      return initialWritingToolsState;
    case "FAIL":
      return { ...state, mode: "error", error: event.message, canRetry: event.canRetry ?? false };
    case "BACK":
      return backState(state);
    case "CLOSE":
      return initialWritingToolsState;
  }
}
