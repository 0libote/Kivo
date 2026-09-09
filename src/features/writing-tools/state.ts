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

export function writingToolsReducer(state: WritingToolsState, event: WritingToolsEvent): WritingToolsState {
  switch (event.type) {
    case "OPEN":
      return {
        ...initialWritingToolsState,
        mode: event.context.hasSelection ? "menu" : "chat",
        context: event.context,
        enabledActions: event.enabledActions,
        sourceText: event.context.initialText ?? "",
      };
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
      return {
        ...state,
        mode: "custom",
        activeAction: "custom",
        customInstruction: event.initialValue ?? state.customInstruction,
        error: null,
      };
    case "OPEN_SUMMARY":
      if (!state.enabledActions.includes("summarize")) return state;
      return {
        ...state,
        mode: "summary",
        activeAction: "summarize",
        summaryKind: event.kind,
        summaryInput: event.kind === "text" ? state.sourceText : (/^https?:\/\/\S+$/i.test(state.sourceText.trim()) ? state.sourceText.trim() : ""),
        usesSummaryInput: true,
        error: null,
      };
    case "SET_SUMMARY_INPUT":
      return state.mode === "summary" ? { ...state, summaryInput: event.value } : state;
    case "SET_CUSTOM":
      return state.mode === "custom" ? { ...state, customInstruction: event.value } : state;
    case "SET_SOURCE":
      return state.mode === "menu" || state.mode === "chat"
        ? { ...state, sourceText: event.value }
        : state;
    case "RUN":
      if (state.mode !== "menu" && state.mode !== "custom" && state.mode !== "chat" && state.mode !== "summary" && state.mode !== "error") return state;
      if ((state.mode === "menu" || state.mode === "custom") && !state.context) return state;
      return { ...state, mode: "processing", activeAction: event.action, error: null, canRetry: false };
    case "RESULT":
      if (state.mode !== "processing") return state;
      return {
        ...state,
        mode: "result",
        resultText: event.text,
        resultSource: event.source,
        resultCanReplace: !(state.usesSummaryInput && state.summaryKind === "link") && (event.canReplace ?? state.context?.canReplace ?? false),
        error: null,
      };
    case "REPLACED":
      return initialWritingToolsState;
    case "FAIL":
      return { ...state, mode: "error", error: event.message, canRetry: event.canRetry ?? false };
    case "BACK":
      if (state.mode === "error" && state.usesSummaryInput) {
        return { ...state, mode: "summary", error: null, canRetry: false };
      }
      if (state.mode === "custom" || state.mode === "summary" || state.mode === "error" || state.mode === "result") {
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
      return state;
    case "CLOSE":
      return initialWritingToolsState;
  }
}
