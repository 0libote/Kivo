import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState, type Dispatch } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { IconButton } from "../../components/IconButton";
import { Spinner } from "../../components/Spinner";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import { NativeError, type AppSettings, type NativeErrorShape, type Platform, type SelectionContext, type SummarySource, type WritingPreset, type WritingRequest } from "../../types";
import { SafeMarkdown } from "./SafeMarkdown";
import { builtinActionFor, resolvePresetPrompt, resolveWritingPresets } from "./presets";
import { initialWritingToolsState, writingToolsReducer, type WritingToolsEvent, type WritingToolsState } from "./state";

interface WritingToolsPopupProps {
  readonly platform: Platform;
  readonly settings: AppSettings;
}

type RunAction = (presetId: string) => Promise<void>;
type PopupDispatch = Dispatch<WritingToolsEvent>;

function getActiveDefinition(activeAction: string | null, presets: WritingPreset[]): WritingPreset | null {
  if (activeAction === null) return null;
  return presets.find((preset) => preset.id === activeAction) ?? null;
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
  enabledActions: string[],
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
  readonly enabledActions: string[];
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
  const presets = useMemo(
    () => resolveWritingPresets(settings),
    [settings.enabledWritingActions, settings.writingPresets],
  );

  const openWithContext = useCallback(
    (context: SelectionContext) => {
      requestGeneration.current += 1;
      requestInFlight.current = false;
      dispatch({ type: "OPEN", context, enabledActions: presets.map((preset) => preset.id) });
    },
    [presets],
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
    const reportHeight = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        // Ask for the full content height; the native shell clamps it to the
        // user's maximum, while the harness can still observe menu changes.
        const content = element.querySelector<HTMLElement>(".writing-menu") ?? element;
        void nativeBridge.setSurfaceMode("writing-tools", mode, content.scrollHeight + 4).catch(() => {});
      });
    };
    const observer = new ResizeObserver(reportHeight);
    const mutationObserver = new MutationObserver(reportHeight);
    observer.observe(element);
    mutationObserver.observe(element, { attributes: true, childList: true, subtree: true });
    reportHeight();
    return () => { observer.disconnect(); mutationObserver.disconnect(); cancelAnimationFrame(frame); };
  }, [state.mode]);

  const close = useCallback(() => {
    requestGeneration.current += 1;
    requestInFlight.current = false;
    dispatch({ type: "CLOSE" });
    // Hiding the native window is best-effort after local state already
    // closed; a failure must surface instead of leaving the spinner view.
    void nativeBridge.closeSurface("writing-tools").catch((error: unknown) => {
      dispatch({ type: "FAIL", message: messageForError(error) });
    });
  }, []);

  const runAction = useCallback(async (presetId: string) => {
    if (requestInFlight.current || state.mode === "closed" || state.mode === "processing") return;
    if (presetId === "summarize" && !summarizeEnabled) return;
    const preset = presets.find((candidate) => candidate.id === presetId);
    if (!preset) return;
    if (presetId === "custom" && state.mode !== "custom" && state.mode !== "error") {
      dispatch({ type: "OPEN_CUSTOM" });
      return;
    }
    const request = prepareWritingRequest(state, preset);
    if (!request) return;

    const generation = ++requestGeneration.current;
    requestInFlight.current = true;

    dispatch({ type: "RUN", action: presetId });
    try {
      const response = await nativeBridge.runWritingAction(request);
      if (generation !== requestGeneration.current) return;
      if (response.kind === "result" && typeof response.text === "string") {
        dispatch({ type: "RESULT", text: response.text, source: response.source, canReplace: response.canReplace });
      } else {
        dispatch({ type: "REPLACED" });
        // Close is best-effort: if hiding fails, show the error instead of a
        // stuck spinner.
        void nativeBridge.closeSurface("writing-tools").catch((error: unknown) => {
          dispatch(failureForError(error));
        });
      }
    } catch (error) {
      if (generation !== requestGeneration.current) return;
      dispatch(failureForError(error));
    } finally {
      if (generation === requestGeneration.current) requestInFlight.current = false;
    }
  }, [state, summarizeEnabled, presets]);

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
    <main className="writing-stage" data-platform={platform}>
      <dialog
        aria-label="Writing Tools"
        className="writing-popup"
        data-mode={state.mode}
        onCancel={(event) => event.preventDefault()}
        open
        ref={popup}
      >
        <PopupContent state={state} presets={presets} close={close} dispatch={dispatch} runAction={runAction} />
      </dialog>
    </main>
  );
}

interface PopupContentProps {
  readonly state: WritingToolsState;
  readonly presets: WritingPreset[];
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
}

function PopupContent({ state, presets, close, dispatch, runAction }: PopupContentProps) {
  const activeDefinition = getActiveDefinition(state.activeAction, presets);
  switch (state.mode) {
    case "closed":
      return <OpeningView />;
    case "menu":
      return (
        <MenuView
          presets={presets}
          applicationName={state.context?.applicationName}
          close={close}
          dispatch={dispatch}
          runAction={runAction}
          selectedIndex={state.selectedIndex}
        />
      );
    case "summary":
      return <LinkSummaryView close={close} dispatch={dispatch} runAction={runAction} url={state.sourceText.trim()} />;
    case "custom":
      return <CustomView customInstruction={state.customInstruction} dispatch={dispatch} runAction={runAction} />;
    case "processing":
      return <ProcessingView close={close} label={state.isLinkSummary ? "Summarizing link" : activeDefinition?.label} hint={state.isLinkSummary ? "Retrieving the page content…" : undefined} />;
    case "result":
      return (
        <ResultView
          close={close}
          canReplace={state.resultCanReplace}
          dispatch={dispatch}
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
          hasContext={state.context?.hasSelection ?? false}
          message={state.error ?? ""}
          runAction={runAction}
        />
      );
    default:
      return null;
  }
}

interface MenuViewProps {
  readonly presets: WritingPreset[];
  readonly applicationName: string | undefined;
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
  readonly selectedIndex: number;
}

function MenuView(props: MenuViewProps) {
  const { presets, applicationName, close, dispatch, runAction, selectedIndex } = props;
  const renderAction = (action: WritingPreset, index: number) => <button
    aria-label={action.label} className="writing-action" data-selected={index === selectedIndex}
    key={action.id} onClick={() => void runAction(action.id)} onFocus={() => dispatch({ type: "SELECT", index })}
    role="menuitem" type="button"><Icon name={action.icon} size={16} /><span className="writing-action__copy"><strong>{action.label}</strong><small>{action.description}</small></span></button>;
  return (
    <div className="writing-menu">
      <div className="writing-popup__top" data-tauri-drag-region>
        <span className="writing-popup__heading">
          <span className="writing-popup__eyebrow">Writing Tools</span>
          <span className="writing-popup__context">{applicationName ? `Selected text in ${applicationName}` : "Selected text"}</span>
        </span>
        <IconButton label="Close Writing Tools" onClick={close}>
          <Icon name="close" size={14} />
        </IconButton>
      </div>
      <div aria-label="Writing actions" className="writing-actions" role="menu">
        {presets.map((preset, index) => renderAction(preset, index))}
      </div>
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
      <IconButton
        className="custom-instruction__back"
        label="Back to writing actions"
        onClick={() => dispatch({ type: "BACK" })}
      >
        <Icon name="arrow-left" size={15} />
      </IconButton>
      <input
        aria-label="Custom writing instruction"
        autoComplete="off"
        onChange={(event) => dispatch({ type: "SET_CUSTOM", value: event.target.value })}
        placeholder="Describe your change"
        ref={customInputRef}
        required
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

function ProcessingView({ close, label, hint }: { readonly close: () => void; readonly label: string | undefined; readonly hint?: string }) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const statusLabel = label ?? "Working";
  const status = elapsed > 0 ? `${statusLabel} · ${elapsed}s` : statusLabel;
  return (
    <div aria-live="polite" className="writing-processing">
      <Spinner label={`Running ${label ?? "writing action"}`} />
      <span>{status}</span>
      {hint || elapsed >= 4 ? <span className="writing-processing__hint">{hint ?? "Still working. Closing cancels the request."}</span> : null}
      <IconButton label="Cancel" onClick={close}>
        <Icon name="close" size={14} />
      </IconButton>
    </div>
  );
}

interface ResultViewProps {
  readonly close: () => void;
  readonly canReplace: boolean;
  readonly dispatch: PopupDispatch;
  readonly label: string | undefined;
  readonly resultText: string;
  readonly source: SummarySource | undefined;
}

function ResultView({ close, canReplace, dispatch, label, resultText, source }: ResultViewProps) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  return (
    <div className="writing-result">
      <header className="writing-result__header" data-tauri-drag-region>
        <div>
          <span className="writing-result__eyebrow">Writing Tools</span>
          <h1>{label ?? "Result"}</h1>
        </div>
        <IconButton label="Close result" onClick={close}>
          <Icon name="close" size={14} />
        </IconButton>
      </header>
      {source ? (
        <div className="writing-result__source">
          <span>{source.kind === "youtube" ? "YouTube" : "Website"}</span>
          <span title={source.url}>{source.url}</span>
        </div>
      ) : null}
      <div className="writing-result__body" aria-live="polite">
        <SafeMarkdown>{resultText}</SafeMarkdown>
      </div>
      {copyError ? <p className="writing-result__error" role="alert">{copyError}</p> : null}
      <footer className="writing-result__footer">
        <Button
          compact
          icon={copied ? "check" : "copy"}
          onClick={() => {
            setCopyError(null);
            void nativeBridge.copyText(resultText).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1000);
            }).catch(() => setCopyError("The result could not be copied. Select the text and copy it manually."));
          }}
        >
          {copied ? "Copied" : "Copy"}
        </Button>
        {canReplace ? (
          <Button
            compact
            onClick={() => {
              void nativeBridge.replaceWritingResult(resultText).then(close).catch((error: unknown) => {
                dispatch(failureForError(error));
              });
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
  readonly activeAction: string | null;
  readonly canRetry: boolean;
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly hasContext: boolean;
  readonly message: string;
  readonly runAction: RunAction;
}

function ErrorView({ activeAction, canRetry, close, dispatch, hasContext, message, runAction }: ErrorViewProps) {
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

interface LinkSummaryViewProps {
  readonly close: () => void;
  readonly dispatch: PopupDispatch;
  readonly runAction: RunAction;
  readonly url: string;
}

function LinkSummaryView({ close, dispatch, runAction, url }: LinkSummaryViewProps) {
  const hostname = new URL(url).hostname.replace(/^www\./u, "");
  return (
    <form className="writing-summary" onSubmit={(event) => { event.preventDefault(); void runAction("summarize"); }}>
      <header className="writing-popup__header" data-tauri-drag-region>
        <span>Summarize link</span>
        <IconButton label="Close Writing Tools" onClick={close}><Icon name="close" size={14} /></IconButton>
      </header>
      <div className="writing-summary__link">
        <Icon name="connection" size={17} />
        <span><strong>{hostname}</strong><small title={url}>{url}</small></span>
      </div>
      <footer className="writing-summary__footer">
        <Button compact onClick={() => dispatch({ type: "BACK" })}>Other actions</Button>
        <Button compact tone="primary" type="submit">Summarize</Button>
      </footer>
    </form>
  );
}

function prepareWritingRequest(state: WritingToolsState, preset: WritingPreset): WritingRequest | undefined {
  const typed = state.customInstruction.trim();
  const isQuickCustom = preset.id === "custom";
  if (isQuickCustom && !typed) return undefined;
  const text = state.sourceText.trim();
  if (!text) return undefined;
  let sourceKind: WritingRequest["sourceKind"];
  if (preset.id === "summarize") sourceKind = state.isLinkSummary ? "link" : "text";
  // The quick "custom" entry folds the typed text into the preset's
  // instruction, so an edited custom template still applies.
  const effective: WritingPreset = isQuickCustom ? { ...preset, instruction: typed } : preset;
  return {
    action: builtinActionFor(preset.id),
    presetId: preset.id,
    instruction: isQuickCustom ? typed : undefined,
    systemInstruction: resolvePresetPrompt(effective),
    replacesSelection: preset.replacesSelection,
    models: preset.models,
    text,
    sourceKind,
  };
}
