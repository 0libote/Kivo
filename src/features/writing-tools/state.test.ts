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

describe("summary inputs", () => {
  function open(hasSelection = false) {
    return writingToolsReducer(initialWritingToolsState, { type: "OPEN", context: hasSelection ? context : chatContext, enabledActions: [...actions] });
  }

  it("preserves quick chat text when entering and leaving pasted summaries", () => {
    const chat = writingToolsReducer(open(), { type: "SET_SOURCE", value: "My draft question" });
    const summary = writingToolsReducer(chat, { type: "OPEN_SUMMARY", kind: "text" });
    const edited = writingToolsReducer(summary, { type: "SET_SUMMARY_INPUT", value: "A long transcript" });
    expect(edited).toMatchObject({ mode: "summary", summaryInput: "A long transcript", sourceText: "My draft question" });
    expect(writingToolsReducer(edited, { type: "BACK" })).toMatchObject({ mode: "chat", sourceText: "My draft question", usesSummaryInput: false });
  });

  it("prefills links only when the selection is a URL", () => {
    expect(writingToolsReducer(open(true), { type: "OPEN_SUMMARY", kind: "link" }).summaryInput).toBe("");
    const selection = writingToolsReducer(open(true), { type: "SET_SOURCE", value: " https://example.com/article " });
    expect(writingToolsReducer(selection, { type: "OPEN_SUMMARY", kind: "link" }).summaryInput).toBe("https://example.com/article");
  });

  it("never offers replacement for a link, even if a response allows it", () => {
    const summary = writingToolsReducer(open(true), { type: "OPEN_SUMMARY", kind: "link" });
    const running = writingToolsReducer(summary, { type: "RUN", action: "summarize" });
    const result = writingToolsReducer(running, { type: "RESULT", text: "Summary", canReplace: true });
    expect(result.resultCanReplace).toBe(false);
  });

  it("honors the backend replacement restriction for text results", () => {
    const running = writingToolsReducer(open(true), { type: "RUN", action: "summarize" });
    expect(writingToolsReducer(running, { type: "RESULT", text: "Summary", canReplace: false }).resultCanReplace).toBe(false);
  });

  it("retries the same summary and returns to its input on Back", () => {
    const summary = writingToolsReducer(open(), { type: "OPEN_SUMMARY", kind: "link" });
    const input = writingToolsReducer(summary, { type: "SET_SUMMARY_INPUT", value: "https://example.com" });
    const running = writingToolsReducer(input, { type: "RUN", action: "summarize" });
    const failed = writingToolsReducer(running, { type: "FAIL", message: "Try again", canRetry: true });
    expect(writingToolsReducer(failed, { type: "BACK" })).toMatchObject({ mode: "summary", summaryInput: "https://example.com" });
    const retried = writingToolsReducer(failed, { type: "RUN", action: "summarize" });
    expect(retried.mode).toBe("processing");
    expect(writingToolsReducer(retried, { type: "RESULT", text: "Summary" }).mode).toBe("result");
  });

  it("respects disabled Summarize and resets summary state on reopen", () => {
    const disabled = writingToolsReducer(initialWritingToolsState, { type: "OPEN", context, enabledActions: ["proofread"] });
    expect(writingToolsReducer(disabled, { type: "OPEN_SUMMARY", kind: "link" })).toBe(disabled);
    const summary = writingToolsReducer(open(), { type: "OPEN_SUMMARY", kind: "link" });
    expect(writingToolsReducer(summary, { type: "OPEN", context, enabledActions: [...actions] })).toMatchObject({ mode: "menu", usesSummaryInput: false, summaryInput: "", resultCanReplace: false });
  });
});
