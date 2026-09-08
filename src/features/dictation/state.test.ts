import { describe, expect, it } from "vitest";
import { dictationReducer, initialDictationState } from "./state";

describe("dictationReducer", () => {
  it("moves through a successful hold-to-dictate session", () => {
    const listening = dictationReducer(initialDictationState, { type: "LISTEN", sessionId: "session-1" });
    expect(listening).toMatchObject({ status: "listening", sessionId: "session-1" });

    const level = dictationReducer(listening, { type: "LEVEL", level: 2 });
    expect(level.level).toBe(1);

    const processing = dictationReducer(level, { type: "PROCESS" });
    expect(processing).toMatchObject({ status: "processing", level: 0 });

    const success = dictationReducer(processing, { type: "SUCCEED" });
    expect(success.status).toBe("success");
    expect(dictationReducer(success, { type: "HIDE" })).toEqual(initialDictationState);
  });

  it("cancels immediately without retaining transcript state", () => {
    const listening = dictationReducer(initialDictationState, { type: "LISTEN" });
    expect(dictationReducer(listening, { type: "CANCEL" })).toEqual(initialDictationState);
  });

  it("returns to the passive idle indicator without retaining session state", () => {
    const listening = dictationReducer(initialDictationState, { type: "LISTEN", sessionId: "session-2" });
    expect(dictationReducer(listening, { type: "IDLE" })).toEqual({
      ...initialDictationState,
      status: "idle",
    });
  });

  it("ignores stale transitions and records recoverable failures", () => {
    expect(dictationReducer(initialDictationState, { type: "PROCESS" })).toEqual(initialDictationState);
    expect(dictationReducer(initialDictationState, { type: "FAIL", message: "No microphone", canRetry: true }))
      .toMatchObject({ status: "error", message: "No microphone", canRetry: true });
  });
});
