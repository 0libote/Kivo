import type { SelectionContext, SummarySource, WritingActionId } from "../../types";

export type WritingMode = "closed" | "menu" | "custom" | "summary" | "processing" | "result" | "error";

export interface WritingToolsState {
  mode: WritingMode;
  context: SelectionContext | null;
  enabledActions: WritingActionId[];
  selectedIndex: number;
  activeAction: WritingActionId | null;
  customInstruction: string;
  sourceText: string;
  isLinkSummary: boolean;
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
  | { type: "SET_CUSTOM"; value: string }
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
  isLinkSummary: false,
  resultCanReplace: false,
  resultText: "",
  error: null,
  canRetry: false,
};

export function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return (url.protocol === "http:" || url.protocol === "https:") && !url.username && !url.password;
  } catch {
    return false;
  }
}

function openState(event: Extract<WritingToolsEvent, { type: "OPEN" }>): WritingToolsState {
  const sourceText = event.context.initialText ?? "";
  if (!event.context.hasSelection || !sourceText.trim()) {
    return {
      ...initialWritingToolsState,
      mode: "error",
      context: event.context,
      enabledActions: event.enabledActions,
      error: "Select some text first.",
    };
  }
  const isLinkSummary = event.enabledActions.includes("summarize") && isWebUrl(sourceText);
  return {
    ...initialWritingToolsState,
    mode: isLinkSummary ? "summary" : "menu",
    context: event.context,
    enabledActions: event.enabledActions,
    sourceText,
    activeAction: isLinkSummary ? "summarize" : null,
    isLinkSummary,
  };
}

function backState(state: WritingToolsState): WritingToolsState {
  if (!state.context?.hasSelection) return state;
  return {
    ...state,
    mode: "menu",
    activeAction: null,
    isLinkSummary: false,
    resultSource: undefined,
    resultCanReplace: false,
    resultText: "",
    error: null,
    canRetry: false,
  };
}

export function writingToolsReducer(state: WritingToolsState, event: WritingToolsEvent): WritingToolsState {
  switch (event.type) {
    case "OPEN":
      return openState(event);
    case "MOVE": {
      if (state.mode !== "menu" || state.enabledActions.length === 0) return state;
      const length = state.enabledActions.length;
      return { ...state, selectedIndex: (state.selectedIndex + event.delta + length) % length };
    }
    case "SELECT":
      return state.mode === "menu"
        ? { ...state, selectedIndex: Math.min(Math.max(0, event.index), Math.max(0, state.enabledActions.length - 1)) }
        : state;
    case "OPEN_CUSTOM":
      return state.context?.hasSelection
        ? { ...state, mode: "custom", activeAction: "custom", customInstruction: event.initialValue ?? state.customInstruction, error: null }
        : state;
    case "SET_CUSTOM":
      return state.mode === "custom" ? { ...state, customInstruction: event.value } : state;
    case "RUN":
      return ["menu", "custom", "summary", "error"].includes(state.mode) && state.context?.hasSelection
        ? { ...state, mode: "processing", activeAction: event.action, error: null, canRetry: false }
        : state;
    case "RESULT":
      return state.mode === "processing"
        ? {
            ...state,
            mode: "result",
            resultText: event.text,
            resultSource: event.source,
            resultCanReplace: !state.isLinkSummary && (event.canReplace ?? state.context?.canReplace ?? false),
            error: null,
          }
        : state;
    case "REPLACED":
    case "CLOSE":
      return initialWritingToolsState;
    case "FAIL":
      return { ...state, mode: "error", error: event.message, canRetry: event.canRetry ?? false };
    case "BACK":
      return backState(state);
  }
}
