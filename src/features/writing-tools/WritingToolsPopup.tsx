import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { Spinner } from "../../components/Spinner";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import { NativeError, type AppSettings, type NativeErrorShape, type Platform, type SelectionContext, type WritingActionId } from "../../types";
import { SafeMarkdown } from "./SafeMarkdown";
import { WRITING_ACTIONS, writingAction } from "./actions";
import { initialWritingToolsState, writingToolsReducer } from "./state";

interface WritingToolsPopupProps {
  platform: Platform;
  settings: AppSettings;
}

export function WritingToolsPopup({ platform, settings }: WritingToolsPopupProps) {
  const [state, dispatch] = useReducer(writingToolsReducer, initialWritingToolsState);
  const [copied, setCopied] = useState(false);
  const customInputRef = useRef<HTMLInputElement>(null);
  const chatInputRef = useRef<HTMLTextAreaElement>(null);
  const summaryInputRef = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const requestGeneration = useRef(0);
  const requestInFlight = useRef(false);
  const summarizeEnabled = settings.enabledWritingActions.includes("summarize");
  const actions = useMemo(
    () => WRITING_ACTIONS.filter((action) => settings.enabledWritingActions.includes(action.id)),
    [settings.enabledWritingActions],
  );

  const openWithContext = useCallback(
    (context: SelectionContext) => {
      requestGeneration.current += 1;
      requestInFlight.current = false;
      setCopied(false);
      dispatch({ type: "OPEN", context, enabledActions: actions.map((action) => action.id) });
    },
    [actions],
  );

  useNativeEvent<SelectionContext>("writing-context", openWithContext);
  useNativeEvent<NativeErrorShape>("writing-error", (error) => {
    dispatch({ type: "FAIL", message: error.message, canRetry: error.recoverable });
  });

  useEffect(() => () => {
    requestGeneration.current += 1;
    requestInFlight.current = false;
  }, []);

  useEffect(() => {
    if (nativeBridge.isNative) return;
    let active = true;
    void nativeBridge
      .getWritingContext()
      .then((context) => active && openWithContext(context))
      .catch((error: unknown) => {
        if (!active) return;
        dispatch({ type: "FAIL", message: messageForError(error) });
      });
    return () => {
      active = false;
    };
  }, [openWithContext]);

  useEffect(() => {
    if (state.mode === "custom") customInputRef.current?.focus();
    if (state.mode === "chat") chatInputRef.current?.focus();
    if (state.mode === "summary") summaryInputRef.current?.focus();
  }, [state.mode]);

  useEffect(() => {
    if (state.mode === "closed") return;
    void nativeBridge.setSurfaceMode("writing-tools", state.mode);
  }, [state.mode]);

  const close = useCallback(() => {
    requestGeneration.current += 1;
    requestInFlight.current = false;
    dispatch({ type: "CLOSE" });
    void nativeBridge.closeSurface("writing-tools");
  }, []);

  const runAction = useCallback(async (actionId: WritingActionId) => {
    if (requestInFlight.current || state.mode === "closed" || state.mode === "processing") return;
    if (actionId === "summarize" && !summarizeEnabled) return;
    if (actionId === "custom" && state.mode !== "custom" && state.mode !== "error") {
      dispatch({ type: "OPEN_CUSTOM" });
      return;
    }
    if (actionId === "custom" && !state.customInstruction.trim()) return;
    const text = (state.usesSummaryInput ? state.summaryInput : state.sourceText).trim();
    if ((actionId === "chat" || state.usesSummaryInput) && !text) return;
    if (state.usesSummaryInput && state.summaryKind === "link" && !isWebUrl(text)) return;

    const generation = ++requestGeneration.current;
    requestInFlight.current = true;

    dispatch({ type: "RUN", action: actionId });
    try {
      const response = await nativeBridge.runWritingAction({
        action: actionId,
        instruction: actionId === "custom" ? state.customInstruction.trim() : undefined,
        text,
        sourceKind: actionId === "summarize" ? (state.usesSummaryInput ? state.summaryKind : "text") : undefined,
      });
      if (generation !== requestGeneration.current) return;
      if (response.kind === "result" && typeof response.text === "string") {
        dispatch({ type: "RESULT", text: response.text, source: response.source, canReplace: response.canReplace });
      } else {
        dispatch({ type: "REPLACED" });
        void nativeBridge.closeSurface("writing-tools");
      }
    } catch (error) {
      if (generation !== requestGeneration.current) return;
      const nativeError = error instanceof NativeError ? error : null;
      dispatch({ type: "FAIL", message: messageForError(error), canRetry: nativeError?.recoverable });
    } finally {
      if (generation === requestGeneration.current) requestInFlight.current = false;
    }
  }, [state.customInstruction, state.mode, state.sourceText, state.summaryInput, state.summaryKind, state.usesSummaryInput, summarizeEnabled]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        if (state.mode === "menu" || state.mode === "processing" || state.mode === "chat") close();
        else dispatch({ type: "BACK" });
        return;
      }
      if (state.mode === "chat") {
        if (event.key === "Enter" && !event.shiftKey) {
          const target = event.target as HTMLElement | null;
          if (target?.tagName === "TEXTAREA") {
            event.preventDefault();
            void runAction("chat");
          }
        }
        return;
      }
      if (state.mode !== "menu") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("button:not(.writing-action)")) return;
      if (target?.matches("input, textarea")) return;

      const movement: Record<string, number> = {
        ArrowRight: 1,
        ArrowLeft: -1,
        ArrowDown: 2,
        ArrowUp: -2,
      };
      if (event.key in movement) {
        event.preventDefault();
        dispatch({ type: "MOVE", delta: movement[event.key] });
      } else if (event.key === "Enter") {
        event.preventDefault();
        const action = state.enabledActions[state.selectedIndex];
        if (action) void runAction(action);
      } else if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        dispatch({ type: "OPEN_CUSTOM", initialValue: event.key });
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close, runAction, state.enabledActions, state.mode, state.selectedIndex]);

  const activeDefinition = state.activeAction
    ? state.activeAction === "chat"
      ? { label: "Quick chat" }
      : writingAction(state.activeAction)
    : null;

  return (
    <main className="writing-stage" data-platform={platform}>
      <section
        aria-label="Writing Tools"
        className="writing-popup"
        data-mode={state.mode}
        role="dialog"
      >
        {state.mode === "closed" ? null : (
          <>
            {state.mode === "menu" ? (
              <div className="writing-menu">
                <header className="writing-popup__header" data-tauri-drag-region>
                  <span>Writing Tools</span>
                  <button aria-label="Close Writing Tools" className="icon-button" onClick={close} type="button">
                    <Icon name="close" size={14} />
                  </button>
                </header>
                {settings.writingAllowManualText ? (
                  <div className="writing-source">
                    <label htmlFor="writing-source-text">
                      {state.context?.applicationName ? `Selected in ${state.context.applicationName}` : "Selected text"}
                    </label>
                    <textarea
                      id="writing-source-text"
                      onChange={(event) => dispatch({ type: "SET_SOURCE", value: event.target.value })}
                      rows={3}
                      spellCheck
                      value={state.sourceText}
                    />
                  </div>
                ) : null}
                <div aria-label="Writing actions" className="writing-actions" role="listbox">
                  {actions.map((action, index) => (
                    <button
                      aria-selected={index === state.selectedIndex}
                      className="writing-action"
                      data-selected={index === state.selectedIndex}
                      key={action.id}
                      onClick={() => void runAction(action.id)}
                      onFocus={() => dispatch({ type: "SELECT", index })}
                      role="option"
                      type="button"
                    >
                      <Icon name={action.icon} size={16} />
                      <span>{action.label}</span>
                    </button>
                  ))}
                </div>
                {summarizeEnabled ? (
                  <div className="writing-summary-actions">
                    <button className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "link" })} type="button">Summarize link…</button>
                  </div>
                ) : null}
                <button className="custom-prompt" onClick={() => dispatch({ type: "OPEN_CUSTOM" })} type="button">
                  <Icon name="pencil" size={15} />
                  <span>Describe your change…</span>
                  <kbd>↵</kbd>
                </button>
              </div>
            ) : null}

            {state.mode === "chat" ? (
              <form
                className="writing-chat"
                onSubmit={(event) => {
                  event.preventDefault();
                  void runAction("chat");
                }}
              >
                <header className="writing-popup__header" data-tauri-drag-region>
                  <span>Quick chat</span>
                  <button aria-label="Close Writing Tools" className="icon-button" onClick={close} type="button">
                    <Icon name="close" size={14} />
                  </button>
                </header>
                <p className="writing-chat__hint">Nothing selected — ask anything.</p>
                <textarea
                  aria-label="Chat message"
                  onChange={(event) => dispatch({ type: "SET_SOURCE", value: event.target.value })}
                  placeholder="Ask anything…"
                  ref={chatInputRef}
                  rows={4}
                  spellCheck
                  value={state.sourceText}
                />
                <div className="writing-chat__footer">
                  {summarizeEnabled ? (
                    <div className="writing-summary-actions">
                      <button className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "text" })} type="button">Summarize text…</button>
                      <button className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "link" })} type="button">Summarize link…</button>
                    </div>
                  ) : null}
                  <Button compact disabled={!state.sourceText.trim()} tone="primary" type="submit">Ask</Button>
                </div>
              </form>
            ) : null}

            {state.mode === "summary" ? (
              <form className="writing-summary" onSubmit={(event) => { event.preventDefault(); void runAction("summarize"); }}>
                <header className="writing-popup__header" data-tauri-drag-region>
                  <span>{state.summaryKind === "link" ? "Summarize link" : "Summarize text"}</span>
                  <button aria-label="Close Writing Tools" className="icon-button" onClick={close} type="button"><Icon name="close" size={14} /></button>
                </header>
                <div className="writing-summary__input">
                  <label htmlFor="summary-input">{state.summaryKind === "link" ? "Webpage or YouTube URL" : "Webpage text or video transcript"}</label>
                  {state.summaryKind === "link" ? (
                    <input id="summary-input" type="url" autoComplete="off" spellCheck={false} placeholder="https://…" ref={(element) => { summaryInputRef.current = element; }} value={state.summaryInput} onChange={(event) => dispatch({ type: "SET_SUMMARY_INPUT", value: event.target.value })} />
                  ) : (
                    <textarea id="summary-input" rows={5} placeholder="Paste text to summarize…" ref={(element) => { summaryInputRef.current = element; }} value={state.summaryInput} onChange={(event) => dispatch({ type: "SET_SUMMARY_INPUT", value: event.target.value })} />
                  )}
                  {state.summaryKind === "link" ? <p>Public pages and YouTube videos. The link is sent to Gemini to retrieve and summarize its content.</p> : null}
                </div>
                <footer className="writing-summary__footer">
                  <Button compact onClick={() => dispatch({ type: "BACK" })}>Back</Button>
                  <Button compact tone="primary" type="submit" disabled={!summarizeEnabled || !state.summaryInput.trim() || (state.summaryKind === "link" && !isWebUrl(state.summaryInput.trim()))}>Summarize</Button>
                </footer>
              </form>
            ) : null}

            {state.mode === "custom" ? (
              <form
                className="custom-instruction"
                onSubmit={(event) => {
                  event.preventDefault();
                  void runAction("custom");
                }}
              >
                <button aria-label="Back to writing actions" className="icon-button custom-instruction__back" onClick={() => dispatch({ type: "BACK" })} type="button">‹</button>
                <input
                  aria-label="Custom writing instruction"
                  autoComplete="off"
                  onChange={(event) => dispatch({ type: "SET_CUSTOM", value: event.target.value })}
                  placeholder="Describe your change"
                  ref={customInputRef}
                  spellCheck
                  value={state.customInstruction}
                />
                <button aria-label="Run instruction" className="custom-instruction__submit" disabled={!state.customInstruction.trim()} type="submit">↵</button>
              </form>
            ) : null}

            {state.mode === "processing" ? (
              <div aria-live="polite" className="writing-processing">
                <Spinner label={`Running ${activeDefinition?.label ?? "writing action"}`} />
                <span>{state.usesSummaryInput && state.summaryKind === "link" ? "Retrieving and summarizing…" : activeDefinition?.label ?? "Working"}</span>
                <button aria-label="Cancel" className="icon-button" onClick={close} type="button"><Icon name="close" size={14} /></button>
              </div>
            ) : null}

            {state.mode === "result" ? (
              <div className="writing-result">
                <header className="writing-result__header" data-tauri-drag-region>
                  <div>
                    <span className="writing-result__eyebrow">Writing Tools</span>
                    <h1>{activeDefinition?.label ?? "Result"}</h1>
                  </div>
                  <button aria-label="Close result" className="icon-button" onClick={close} type="button"><Icon name="close" size={14} /></button>
                </header>
                {state.resultSource ? <div className="writing-result__source"><span>{state.resultSource.kind === "youtube" ? "YouTube" : "Website"}</span><span title={state.resultSource.url}>{state.resultSource.url}</span></div> : null}
                <div className="writing-result__body"><SafeMarkdown>{state.resultText}</SafeMarkdown></div>
                <footer className="writing-result__footer">
                  <Button
                    compact
                    icon={copied ? "check" : "copy"}
                    onClick={() => {
                      void nativeBridge.copyText(state.resultText).then(() => {
                        setCopied(true);
                        window.setTimeout(() => setCopied(false), 1000);
                      });
                    }}
                  >
                    {copied ? "Copied" : "Copy"}
                  </Button>
                  {state.resultCanReplace ? (
                    <Button
                      compact
                      onClick={() => {
                        void nativeBridge.replaceWritingResult(state.resultText).then(close);
                      }}
                      tone="primary"
                    >
                      Replace
                    </Button>
                  ) : null}
                </footer>
              </div>
            ) : null}

            {state.mode === "error" ? (
              <div aria-live="assertive" className="writing-error">
                <Icon name="error" size={20} />
                <div><strong>Writing Tools</strong><span>{state.error}</span></div>
                <div className="writing-error__actions">
                  {state.canRetry && state.activeAction ? <Button compact onClick={() => void runAction(state.activeAction!)}>Retry</Button> : null}
                  {state.usesSummaryInput && state.summaryKind === "link" ? <Button compact onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "text" })}>Paste text instead</Button> : null}
                  <Button compact onClick={state.context ? () => dispatch({ type: "BACK" }) : close}>{state.context ? "Back" : "Close"}</Button>
                </div>
              </div>
            ) : null}
          </>
        )}
      </section>
    </main>
  );
}

function messageForError(error: unknown) {
  if (error instanceof NativeError) return error.message;
  return "Writing Tools couldn’t complete that request.";
}

function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (url.protocol === "http:" || url.protocol === "https:") && !url.username && !url.password;
  } catch {
    return false;
  }
}
