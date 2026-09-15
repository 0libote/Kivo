import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type CSSProperties, type Dispatch } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { Spinner } from "../../components/Spinner";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import { NativeError, type AppSettings, type NativeErrorShape, type Platform, type SelectionContext, type SummarySource, type WritingActionId, type WritingRequest } from "../../types";
import { SafeMarkdown } from "./SafeMarkdown";
import { WRITING_ACTIONS, writingAction } from "./actions";
import { initialWritingToolsState, writingToolsReducer, type WritingToolsEvent, type WritingToolsState } from "./state";

interface WritingToolsPopupProps {
  readonly platform: Platform;
  readonly settings: AppSettings;
}

type RunAction = (actionId: WritingActionId) => Promise<void>;
type PopupDispatch = Dispatch<WritingToolsEvent>;

function getActiveDefinition(activeAction: WritingActionId | null): { label: string } | null {
  if (activeAction === null) return null;
  return writingAction(activeAction);
}

const MENU_MOVEMENT: Readonly<Record<string, number>> = {
  ArrowDown: 1,
  ArrowRight: 1,
  ArrowUp: -1,
  ArrowLeft: -1,
};

function isClosableMode(mode: WritingToolsState["mode"]): boolean {
  return mode === "menu" || mode === "processing";
}

function handleEscape(
  mode: WritingToolsState["mode"],
  close: () => void,
  dispatch: PopupDispatch,
): void {
  if (isClosableMode(mode)) {
    close();
  } else {
    dispatch({ type: "BACK" });
  }
}

function handleMenuKey(
  event: KeyboardEvent,
  enabledActions: WritingActionId[],
  selectedIndex: number,
  runAction: RunAction,
  dispatch: PopupDispatch,
): void {
  const target = event.target as HTMLElement | null;
  if (target?.matches("input, textarea, summary") || target?.closest("button:not(.writing-action)")) return;
  const delta = MENU_MOVEMENT[event.key];
  if (delta !== undefined) {
    event.preventDefault();
    dispatch({ type: "MOVE", delta });
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    const action = enabledActions[selectedIndex];
    if (action) void runAction(action);
    return;
  }
  if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
    event.preventDefault();
    dispatch({ type: "OPEN_CUSTOM", initialValue: event.key });
  }
}

function useWritingHotkeys(options: {
  readonly mode: WritingToolsState["mode"];
  readonly hasSelection: boolean;
  readonly enabledActions: WritingActionId[];
  readonly selectedIndex: number;
  readonly close: () => void;
  readonly runAction: RunAction;
  readonly dispatch: PopupDispatch;
}): void {
  const { mode, hasSelection, enabledActions, selectedIndex, close, runAction, dispatch } = options;
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        // With no selection there is no menu to go back to, so Escape
        // always closes instead of landing on a dead-end entry.
        if (!hasSelection) {
          close();
        } else {
          handleEscape(mode, close, dispatch);
        }
        return;
      }
      if (mode !== "menu") return;
      handleMenuKey(event, enabledActions, selectedIndex, runAction, dispatch);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close, runAction, enabledActions, hasSelection, mode, selectedIndex, dispatch]);
}

export function WritingToolsPopup({ platform, settings }: WritingToolsPopupProps) {
  const [state, dispatch] = useReducer(writingToolsReducer, initialWritingToolsState);
  const requestGeneration = useRef(0);
  const requestInFlight = useRef(false);
  const popup = useRef<HTMLDialogElement>(null);
  const summarizeEnabled = settings.enabledWritingActions.includes("summarize");
  const actions = useMemo(
    () => WRITING_ACTIONS.filter((action) => settings.enabledWritingActions.includes(action.id)),
    [settings.enabledWritingActions],
  );

  const openWithContext = useCallback(
    (context: SelectionContext) => {
      requestGeneration.current += 1;
      requestInFlight.current = false;
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

  useLayoutEffect(() => {
    const element = popup.current;
    const mode = state.mode;
    if (mode === "closed" || !element) return;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        // Layout height excludes the entrance animation's temporary scale.
        void nativeBridge.setSurfaceMode("writing-tools", mode, element.offsetHeight + 4).catch(() => {});
      });
    });
    observer.observe(element);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); };
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
    const request = prepareWritingRequest(state, actionId);
    if (!request) return;

    const generation = ++requestGeneration.current;
    requestInFlight.current = true;

    dispatch({ type: "RUN", action: actionId });
    try {
      const response = await nativeBridge.runWritingAction(request);
      if (generation !== requestGeneration.current) return;
      if (response.kind === "result" && typeof response.text === "string") {
        dispatch({ type: "RESULT", text: response.text, source: response.source, canReplace: response.canReplace });
      } else {
        dispatch({ type: "REPLACED" });
        void nativeBridge.closeSurface("writing-tools");
      }
    } catch (error) {
      if (generation !== requestGeneration.current) return;
      dispatch(failureForError(error));
    } finally {
      if (generation === requestGeneration.current) requestInFlight.current = false;
    }
  }, [state, summarizeEnabled]);

  useWritingHotkeys({
    mode: state.mode,
    hasSelection: state.context?.hasSelection ?? false,
    enabledActions: state.enabledActions,
    selectedIndex: state.selectedIndex,
    close,
    runAction,
    dispatch,
  });

  return (
    <main className="writing-stage" data-platform={platform} style={{ "--writing-max-height": `${Math.min(settings.writingPopupHeight, window.screen.availHeight - 32)}px` } as CSSProperties}>
      <dialog
        aria-label="Writing Tools"
        className="writing-popup"
        data-mode={state.mode}
        onCancel={(event) => event.preventDefault()}
        open
        ref={popup}
      >
        <PopupContent state={state} actions={actions} settings={settings} close={close} dispatch={dispatch} runAction={runAction} summarizeEnabled={summarizeEnabled} />
      </dialog>
    </main>
  );
}

interface PopupContentProps {
  readonly state: WritingToolsState;
  readonly actions: typeof WRITING_ACTIONS;
  readonly settings: AppSettings;
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
  readonly summarizeEnabled: boolean;
}

function PopupContent({ state, actions, settings, close, dispatch, runAction, summarizeEnabled }: PopupContentProps) {
  const activeDefinition = getActiveDefinition(state.activeAction);
  switch (state.mode) {
    case "closed":
      return <OpeningView />;
    case "menu":
      return (
        <MenuView
          actions={actions}
          allowManualText={settings.writingAllowManualText}
          applicationName={state.context?.applicationName}
          close={close}
          dispatch={dispatch}
          runAction={runAction}
          selectedIndex={state.selectedIndex}
          sourceText={state.sourceText}
          summarizeEnabled={summarizeEnabled}
        />
      );
    case "summary":
      return <SummaryView close={close} dispatch={dispatch} runAction={runAction} kind={state.summaryKind} input={state.summaryInput} enabled={summarizeEnabled} hasSelection={state.context?.hasSelection ?? false} />;
    case "custom":
      return <CustomView customInstruction={state.customInstruction} dispatch={dispatch} runAction={runAction} />;
    case "processing":
      return <ProcessingView close={close} label={state.usesSummaryInput && state.summaryKind === "link" ? "Retrieving and summarizing…" : activeDefinition?.label} />;
    case "result":
      return (
        <ResultView
          close={close}
          canReplace={state.resultCanReplace}
          source={state.resultSource}
          label={activeDefinition?.label}
          resultText={state.resultText}
        />
      );
    case "error":
      return (
        <ErrorView
          activeAction={state.activeAction}
          canRetry={state.canRetry}
          close={close}
          dispatch={dispatch}
          hasContext={state.context !== null}
          showTextFallback={state.usesSummaryInput && state.summaryKind === "link"}
          message={state.error ?? ""}
          runAction={runAction}
        />
      );
    default:
      return null;
  }
}

interface MenuViewProps {
  readonly actions: typeof WRITING_ACTIONS;
  readonly allowManualText: boolean;
  readonly applicationName: string | undefined;
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
  readonly selectedIndex: number;
  readonly sourceText: string;
  readonly summarizeEnabled: boolean;
}

function MenuView(props: MenuViewProps) {
  const { actions, allowManualText, applicationName, close, dispatch, runAction, selectedIndex, sourceText, summarizeEnabled } = props;
  const more = useRef<HTMLDetailsElement>(null);
  useEffect(() => { if (selectedIndex >= 3 && more.current) more.current.open = true; }, [selectedIndex]);
  const renderAction = (action: typeof actions[number], index: number) => <button
    aria-selected={index === selectedIndex} className="writing-action" data-selected={index === selectedIndex}
    key={action.id} onClick={() => void runAction(action.id)} onFocus={() => dispatch({ type: "SELECT", index })}
    role="option" type="button"><Icon name={action.icon} size={16} /><span>{action.label}</span></button>;
  return (
    <div className="writing-menu">
      <div className="writing-command" data-tauri-drag-region>
        <button className="custom-prompt" onClick={() => dispatch({ type: "OPEN_CUSTOM" })} type="button">
          <Icon name="pencil" size={15} />
          <span>Describe your change…</span>
          <kbd>↵</kbd>
        </button>
        <button aria-label="Close Writing Tools" className="icon-button" onClick={close} type="button">
          <Icon name="close" size={14} />
        </button>
      </div>
      <div aria-label="Writing actions" className="writing-actions" role="listbox">
        {actions.slice(0, 3).map((action, index) => renderAction(action, index))}
        {actions.length > 3 ? <details className="writing-more" ref={more}><summary>More actions</summary><div role="group" aria-label="More writing actions">{actions.slice(3).map((action, index) => renderAction(action, index + 3))}</div></details> : null}
      </div>
      {summarizeEnabled ? <SummaryActions dispatch={dispatch} includeText={false} /> : null}
      {allowManualText ? (
        <div className="writing-source">
          <label htmlFor="writing-source-text">
            {applicationName ? `Selected in ${applicationName}` : "Selected text"}
          </label>
          <textarea
            id="writing-source-text"
            onChange={(event) => dispatch({ type: "SET_SOURCE", value: event.target.value })}
            rows={3}
            spellCheck
            value={sourceText}
          />
        </div>
      ) : null}
      <p className="writing-hint">↑↓ to choose · ↵ to run · Esc to close</p>
    </div>
  );
}

interface CustomViewProps {
  readonly customInstruction: string;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
}

function CustomView({ customInstruction, dispatch, runAction }: CustomViewProps) {
  const customInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    customInputRef.current?.focus();
  }, []);
  return (
    <form
      className="custom-instruction"
      onSubmit={(event) => {
        event.preventDefault();
        void runAction("custom");
      }}
    >
      <button
        aria-label="Back to writing actions"
        className="icon-button custom-instruction__back"
        onClick={() => dispatch({ type: "BACK" })}
        type="button"
      >
        ‹
      </button>
      <input
        aria-label="Custom writing instruction"
        autoComplete="off"
        onChange={(event) => dispatch({ type: "SET_CUSTOM", value: event.target.value })}
        placeholder="Describe your change"
        ref={customInputRef}
        spellCheck
        value={customInstruction}
      />
      <button aria-label="Run instruction" className="custom-instruction__submit" disabled={!customInstruction.trim()} type="submit">
        ↵
      </button>
    </form>
  );
}

function OpeningView() {
  return (
    <div aria-live="polite" className="writing-opening">
      <Spinner label="Opening Writing Tools" />
    </div>
  );
}

function ProcessingView({ close, label }: { readonly close: () => void; readonly label: string | undefined }) {
  return (
    <div aria-live="polite" className="writing-processing">
      <Spinner label={`Running ${label ?? "writing action"}`} />
      <span>{label ?? "Working"}</span>
      <button aria-label="Cancel" className="icon-button" onClick={close} type="button">
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}

interface ResultViewProps {
  readonly close: () => void;
  readonly canReplace: boolean;
  readonly label: string | undefined;
  readonly resultText: string;
  readonly source: SummarySource | undefined;
}

function ResultView({ close, canReplace, label, resultText, source }: ResultViewProps) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="writing-result">
      <header className="writing-result__header" data-tauri-drag-region>
        <div>
          <span className="writing-result__eyebrow">Writing Tools</span>
          <h1>{label ?? "Result"}</h1>
        </div>
        <button aria-label="Close result" className="icon-button" onClick={close} type="button">
          <Icon name="close" size={14} />
        </button>
      </header>
      {source ? (
        <div className="writing-result__source">
          <span>{source.kind === "youtube" ? "YouTube" : "Website"}</span>
          <span title={source.url}>{source.url}</span>
        </div>
      ) : null}
      <div className="writing-result__body">
        <SafeMarkdown>{resultText}</SafeMarkdown>
      </div>
      <footer className="writing-result__footer">
        <Button
          compact
          icon={copied ? "check" : "copy"}
          onClick={() => {
            void nativeBridge.copyText(resultText).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1000);
            });
          }}
        >
          {copied ? "Copied" : "Copy"}
        </Button>
        {canReplace ? (
          <Button
            compact
            onClick={() => {
              void nativeBridge.replaceWritingResult(resultText).then(close);
            }}
            tone="primary"
          >
            Replace
          </Button>
        ) : null}
      </footer>
    </div>
  );
}

interface ErrorViewProps {
  readonly activeAction: WritingActionId | null;
  readonly canRetry: boolean;
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly hasContext: boolean;
  readonly showTextFallback: boolean;
  readonly message: string;
  readonly runAction: RunAction;
}

function ErrorView({ activeAction, canRetry, close, dispatch, hasContext, showTextFallback, message, runAction }: ErrorViewProps) {
  const canShowRetry = canRetry && activeAction !== null;
  return (
    <div aria-live="assertive" className="writing-error">
      <Icon name="error" size={20} />
      <div>
        <strong>Writing Tools</strong>
        <span>{message}</span>
      </div>
      <div className="writing-error__actions">
        {canShowRetry && activeAction ? (
          <Button compact onClick={() => void runAction(activeAction)}>
            Retry
          </Button>
        ) : null}
        {showTextFallback ? <Button compact onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "text" })}>Paste text instead</Button> : null}
        <Button compact onClick={hasContext ? () => dispatch({ type: "BACK" }) : close}>
          {hasContext ? "Back" : "Close"}
        </Button>
      </div>
    </div>
  );
}

function messageForError(error: unknown) {
  if (error instanceof NativeError) return error.message;
  return "Writing Tools couldn’t complete that request.";
}

function failureForError(error: unknown): WritingToolsEvent {
  return {
    type: "FAIL",
    message: messageForError(error),
    canRetry: error instanceof NativeError ? error.recoverable : undefined,
  };
}

function SummaryActions({ dispatch, includeText }: { readonly dispatch: PopupDispatch; readonly includeText: boolean }) {
  return (
    <div className="writing-summary-actions">
      {includeText ? <button className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "text" })} type="button">Summarize text…</button> : null}
      <button className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "link" })} type="button">Summarize link…</button>
    </div>
  );
}

interface SummaryViewProps {
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
  readonly kind: "text" | "link";
  readonly input: string;
  readonly enabled: boolean;
  /** False when the popup opened with nothing selected: there is no menu to
   * go back to, so Back is hidden and a text/link toggle is shown instead. */
  readonly hasSelection: boolean;
}

function SummaryView({ close, dispatch, runAction, kind, input, enabled, hasSelection }: SummaryViewProps) {
  const summaryInputRef = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  useEffect(() => {
    summaryInputRef.current?.focus();
  }, []);
  return (
    <form className="writing-summary" onSubmit={(event) => { event.preventDefault(); void runAction("summarize"); }}>
      <header className="writing-popup__header" data-tauri-drag-region>
        <span>{kind === "link" ? "Summarize link" : "Summarize text"}</span>
        <button aria-label="Close Writing Tools" className="icon-button" onClick={close} type="button"><Icon name="close" size={14} /></button>
      </header>
      {!hasSelection ? (
        <fieldset className="writing-summary__kind">
          <legend>Source</legend>
          <button aria-pressed={kind === "text"} className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "text" })} type="button">Text</button>
          <button aria-pressed={kind === "link"} className="writing-text-action" onClick={() => dispatch({ type: "OPEN_SUMMARY", kind: "link" })} type="button">Link</button>
        </fieldset>
      ) : null}
      <div className="writing-summary__input">
        <label htmlFor="summary-input">{kind === "link" ? "Webpage or YouTube URL" : "Webpage text or video transcript"}</label>
        {kind === "link" ? (
          <input id="summary-input" type="url" autoComplete="off" spellCheck={false} placeholder="https://…" ref={(element) => { summaryInputRef.current = element; }} value={input} onChange={(event) => dispatch({ type: "SET_SUMMARY_INPUT", value: event.target.value })} />
        ) : (
          <textarea id="summary-input" rows={5} placeholder="Paste text to summarize…" ref={(element) => { summaryInputRef.current = element; }} value={input} onChange={(event) => dispatch({ type: "SET_SUMMARY_INPUT", value: event.target.value })} />
        )}
        {kind === "link" ? <p>Public pages and YouTube videos. The link is sent to Gemini to retrieve and summarize its content.</p> : null}
      </div>
      <footer className="writing-summary__footer">
        {hasSelection ? <Button compact onClick={() => dispatch({ type: "BACK" })}>Back</Button> : <span />}
        <Button compact tone="primary" type="submit" disabled={!enabled || !input.trim() || (kind === "link" && !isWebUrl(input.trim()))}>Summarize</Button>
      </footer>
    </form>
  );
}

function prepareWritingRequest(state: WritingToolsState, action: WritingActionId): WritingRequest | undefined {
  if (action === "custom" && !state.customInstruction.trim()) return undefined;
  const text = (state.usesSummaryInput ? state.summaryInput : state.sourceText).trim();
  if (state.usesSummaryInput && !text) return undefined;
  if (state.usesSummaryInput && state.summaryKind === "link" && !isWebUrl(text)) return undefined;
  const sourceKind = state.usesSummaryInput ? state.summaryKind : "text";
  return {
    action,
    instruction: action === "custom" ? state.customInstruction.trim() : undefined,
    text,
    sourceKind: action === "summarize" ? sourceKind : undefined,
  };
}

function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (url.protocol === "http:" || url.protocol === "https:") && !url.username && !url.password;
  } catch {
    return false;
  }
}
