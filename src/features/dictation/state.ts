export type DictationStatus = "hidden" | "idle" | "listening" | "processing" | "success" | "error";

export interface DictationState {
  status: DictationStatus;
  sessionId: string | null;
  level: number;
  message: string | null;
  canRetry: boolean;
}

export type DictationEvent =
  | { type: "IDLE" }
  | { type: "LISTEN"; sessionId?: string }
  | { type: "LEVEL"; level: number }
  | { type: "PROCESS" }
  | { type: "SUCCEED" }
  | { type: "FAIL"; message: string; canRetry?: boolean }
  | { type: "CANCEL" }
  | { type: "HIDE" };

export const initialDictationState: DictationState = {
  status: "hidden",
  sessionId: null,
  level: 0,
  message: null,
  canRetry: false,
};

export function dictationReducer(state: DictationState, event: DictationEvent): DictationState {
  switch (event.type) {
    case "IDLE":
      return { ...initialDictationState, status: "idle" };
    case "LISTEN":
      return {
        status: "listening",
        sessionId: event.sessionId ?? state.sessionId,
        level: 0,
        message: null,
        canRetry: false,
      };
    case "LEVEL":
      if (state.status !== "listening") return state;
      return { ...state, level: Math.min(1, Math.max(0, event.level)) };
    case "PROCESS":
      if (state.status !== "listening") return state;
      return { ...state, status: "processing", level: 0 };
    case "SUCCEED":
      if (state.status !== "processing" && state.status !== "listening") return state;
      return { ...state, status: "success", level: 0 };
    case "FAIL":
      return {
        ...state,
        status: "error",
        level: 0,
        message: event.message,
        canRetry: event.canRetry ?? false,
      };
    case "CANCEL":
    case "HIDE":
      return initialDictationState;
  }
}
