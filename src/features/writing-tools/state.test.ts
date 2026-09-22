import { describe, expect, it } from "bun:test";
import { initialWritingToolsState, isWebUrl, writingToolsReducer } from "./state";

const context = {
  hasSelection: true,
  applicationName: "Editor",
  canReplace: true,
  initialText: "Hello, how are you?",
};
const linkContext = { ...context, initialText: "https://example.com/article" };
const emptyContext = {
  hasSelection: false,
  applicationName: "Editor",
  canReplace: false,
  initialText: "",
};
const actions = ["proofread", "summarize", "custom"] as const;

function open(selected = context) {
  return writingToolsReducer(initialWritingToolsState, {
    type: "OPEN",
    context: selected,
    enabledActions: [...actions],
  });
}

describe("writingToolsReducer", () => {
  it("opens presets for selected text and wraps keyboard navigation", () => {
    const state = open();
    expect(state).toMatchObject({
      mode: "menu",
      sourceText: context.initialText,
      isLinkSummary: false,
    });
    expect(writingToolsReducer(state, { type: "MOVE", delta: -1 }).selectedIndex).toBe(2);
    expect(writingToolsReducer(state, { type: "MOVE", delta: 3 }).selectedIndex).toBe(0);
  });

  it("opens highlighted links directly in link summary mode", () => {
    const state = open(linkContext);
    expect(state).toMatchObject({
      mode: "summary",
      activeAction: "summarize",
      isLinkSummary: true,
    });
    const processing = writingToolsReducer(state, { type: "RUN", action: "summarize" });
    const result = writingToolsReducer(processing, {
      type: "RESULT",
      text: "Summary",
      canReplace: true,
    });
    expect(result).toMatchObject({ mode: "result", resultCanReplace: false });
  });

  it("requires a selection instead of offering a manual AI input", () => {
    expect(open(emptyContext)).toMatchObject({ mode: "error", error: "Select some text first." });
  });

  it("keeps normal summaries replaceable only when the backend allows it", () => {
    const processing = writingToolsReducer(open(), { type: "RUN", action: "summarize" });
    expect(
      writingToolsReducer(processing, { type: "RESULT", text: "Summary", canReplace: false })
        .resultCanReplace,
    ).toBe(false);
    expect(
      writingToolsReducer(processing, { type: "RESULT", text: "Summary", canReplace: true })
        .resultCanReplace,
    ).toBe(true);
  });

  it("dismisses after replacement and keeps errors retryable", () => {
    const processing = writingToolsReducer(open(), { type: "RUN", action: "proofread" });
    expect(writingToolsReducer(processing, { type: "REPLACED" })).toEqual(initialWritingToolsState);
    expect(
      writingToolsReducer(processing, { type: "FAIL", message: "Try again", canRetry: true }),
    ).toMatchObject({ mode: "error", canRetry: true });
  });
});

describe("isWebUrl", () => {
  it("accepts only credential-free HTTP links", () => {
    expect(isWebUrl(" https://example.com/article ")).toBe(true);
    expect(isWebUrl("javascript:alert(1)")).toBe(false);
    expect(isWebUrl("https://user:secret@example.com")).toBe(false);
    expect(isWebUrl("Some selected text")).toBe(false);
  });
});
