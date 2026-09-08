import type { SelectionContext, WritingActionId } from "../../types";

export type WritingMode = "closed" | "menu" | "custom" | "processing" | "result" | "error";

export interface WritingToolsState {
  mode: WritingMode;
  context: SelectionContext | null;
  enabledActions: WritingActionId[];
  selectedIndex: number;
  activeAction: WritingActionId | null;
  customInstruction: string;
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
  | { type: "RESULT"; text: string }
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
  resultText: "",
  error: null,
  canRetry: false,
};

export function writingToolsReducer(state: WritingToolsState, event: WritingToolsEvent): WritingToolsState {
  switch (event.type) {
    case "OPEN":
      return {
        ...initialWritingToolsState,
        mode: "menu",
        context: event.context,
        enabledActions: event.enabledActions,
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
    case "SET_CUSTOM":
      return state.mode === "custom" ? { ...state, customInstruction: event.value } : state;
    case "RUN":
      if (!state.context) return state;
      return { ...state, mode: "processing", activeAction: event.action, error: null, canRetry: false };
    case "RESULT":
      if (state.mode !== "processing") return state;
      return { ...state, mode: "result", resultText: event.text, error: null };
    case "REPLACED":
      return initialWritingToolsState;
    case "FAIL":
      return { ...state, mode: "error", error: event.message, canRetry: event.canRetry ?? false };
    case "BACK":
      if (state.mode === "custom" || state.mode === "error" || state.mode === "result") {
        return { ...state, mode: "menu", activeAction: null, error: null, canRetry: false, resultText: "" };
      }
      return state;
    case "CLOSE":
      return initialWritingToolsState;
  }
}
