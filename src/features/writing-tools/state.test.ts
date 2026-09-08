import { describe, expect, it } from "vitest";
import { initialWritingToolsState, writingToolsReducer } from "./state";

const context = { hasSelection: true, applicationName: "TextEdit", canReplace: true, initialText: "Hello, how are you?" };
const chatContext = { hasSelection: false, applicationName: "Quick chat", canReplace: false, initialText: "" };
const actions = ["proofread", "summarize", "custom"] as const;

describe("writingToolsReducer", () => {
  it("wraps keyboard navigation", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: [...actions] });
    expect(writingToolsReducer(open, { type: "MOVE", delta: -1 }).selectedIndex).toBe(2);
    expect(writingToolsReducer(open, { type: "MOVE", delta: 3 }).selectedIndex).toBe(0);
  });

  it("prefills the editable source box with the captured highlight", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: [...actions] });
    expect(open).toMatchObject({ mode: "menu", sourceText: "Hello, how are you?" });
    const edited = writingToolsReducer(open, { type: "SET_SOURCE", value: "Edited text" });
    expect(edited.sourceText).toBe("Edited text");
  });

  it("opens quick chat when nothing is selected", () => {
    const open = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context: chatContext, enabledActions: [...actions] });
    expect(open).toMatchObject({ mode: "chat", sourceText: "" });
    const processing = writingToolsReducer(open, { type: "RUN", action: "chat" });
    expect(processing.mode).toBe("processing");
    const result = writingToolsReducer(processing, { type: "RESULT", text: "An answer." });
    expect(result).toMatchObject({ mode: "result", activeAction: "chat" });
    expect(writingToolsReducer(result, { type: "BACK" })).toMatchObject({ mode: "chat" });
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
