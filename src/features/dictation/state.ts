import type { DictationSnapshot } from "../../types";

export type DictationStatus =
  | "hidden"
  | "idle"
  | "starting"
  | "listening"
  | "processing"
  | "success"
  | "error";

export interface DictationState {
  status: DictationStatus;
  sessionId: string | null;
  level: number;
  message: string | null;
  canRetry: boolean;
}

export type DictationEvent =
  | { type: "SNAPSHOT"; snapshot: DictationSnapshot }
  | { type: "START" }
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
    case "SNAPSHOT": {
      const { snapshot } = event;
      return {
        ...initialDictationState,
        status: snapshot.status,
        sessionId: snapshot.sessionId ?? null,
        level: snapshot.status === "listening" ? normalizedLevel(snapshot.level ?? 0) : 0,
        message:
          snapshot.status === "error" ? (snapshot.message ?? "Dictation couldn’t finish.") : null,
        canRetry: snapshot.status === "error" && (snapshot.canRetry ?? false),
      };
    }
    case "START":
      return { ...initialDictationState, status: "starting" };
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
      return { ...state, level: normalizedLevel(event.level) };
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

function normalizedLevel(level: number): number {
  return Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0;
}
