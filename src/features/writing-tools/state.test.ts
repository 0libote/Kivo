import { describe, expect, it } from "vitest";
import { initialWritingToolsState, writingToolsReducer } from "./state";

const context = { hasSelection: true, applicationName: "TextEdit", canReplace: true };
const actions = ["proofread", "summarize", "custom"] as const;

describe("writingToolsReducer", () => {
  it("wraps keyboard navigation", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: [...actions] });
    expect(writingToolsReducer(open, { type: "MOVE", delta: -1 }).selectedIndex).toBe(2);
    expect(writingToolsReducer(open, { type: "MOVE", delta: 3 }).selectedIndex).toBe(0);
  });

  it("dismisses after replacement", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: [...actions] });
    const processing = writingToolsReducer(open, { type: "RUN", action: "proofread" });
    expect(processing.mode).toBe("processing");
    expect(writingToolsReducer(processing, { type: "REPLACED" })).toEqual(initialWritingToolsState);
  });

  it("morphs informational actions into a result surface", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: [...actions] });
    const processing = writingToolsReducer(open, { type: "RUN", action: "summarize" });
    const result = writingToolsReducer(processing, { type: "RESULT", text: "A useful summary." });
    expect(result).toMatchObject({ mode: "result", activeAction: "summarize", resultText: "A useful summary." });
  });

  it("keeps errors calm and retryable", () => {
    const failed = writingToolsReducer(initialWritingToolsState, {
      type: "FAIL",
      message: "Gemini is temporarily rate limited.",
      canRetry: true,
    });
    expect(failed).toMatchObject({ mode: "error", canRetry: true });
  });
});
