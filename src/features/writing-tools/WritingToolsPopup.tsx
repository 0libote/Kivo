import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Spinner } from "@astryxdesign/core/Spinner";
import * as stylex from "@stylexjs/stylex";
import { motion } from "motion/react";
import {
  type Dispatch,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { Icon } from "../../components/Icon";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import {
  type AppSettings,
  NativeError,
  type NativeErrorShape,
  type Platform,
  type SelectionContext,
  type SummarySource,
  type WritingPreset,
  type WritingRequest,
} from "../../types";
import { builtinActionFor, resolvePresetPrompt, resolveWritingPresets } from "./presets";
import { SafeMarkdown } from "./SafeMarkdown";
import {
  initialWritingToolsState,
  type WritingToolsEvent,
  type WritingToolsState,
  writingToolsReducer,
} from "./state";

interface WritingToolsPopupProps {
  readonly platform: Platform;
  readonly settings: AppSettings;
}

type RunAction = (presetId: string) => Promise<void>;
type PopupDispatch = Dispatch<WritingToolsEvent>;

function getActiveDefinition(
  activeAction: string | null,
  presets: WritingPreset[],
): WritingPreset | null {
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
  if (
    target?.matches("input, textarea, summary") ||
    target?.closest("button:not([data-writing-action])")
  )
    return;
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
  const presets = useMemo(() => resolveWritingPresets(settings), [settings]);

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

  useEffect(
    () => () => {
      requestGeneration.current += 1;
      requestInFlight.current = false;
    },
    [],
  );

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
        const content =
          element.querySelector<HTMLElement>("[data-writing-menu]") ?? element.firstElementChild;
        void nativeBridge
          .setSurfaceMode(
            "writing-tools",
            mode,
            (content instanceof HTMLElement ? content.scrollHeight : element.scrollHeight) + 4,
          )
          .catch(() => {});
      });
    };
    const observer = new ResizeObserver(reportHeight);
    const mutationObserver = new MutationObserver(reportHeight);
    observer.observe(element);
    mutationObserver.observe(element, { attributes: true, childList: true, subtree: true });
    reportHeight();
    return () => {
      observer.disconnect();
      mutationObserver.disconnect();
      cancelAnimationFrame(frame);
    };
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

  const runAction = useCallback(
    async (presetId: string) => {
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
          dispatch({
            type: "RESULT",
            text: response.text,
            source: response.source,
            canReplace: response.canReplace,
          });
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
    },
    [state, summarizeEnabled, presets],
  );

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
    <main data-platform={platform} {...stylex.props(styles.stage)}>
      <dialog
        aria-label="Writing Tools"
        data-mode={state.mode}
        onCancel={(event) => event.preventDefault()}
        open
        ref={popup}
        {...stylex.props(styles.popup)}
      >
        <motion.div
          animate={{ opacity: 1, y: 0, scale: 1 }}
          initial={{ opacity: 0, y: -3, scale: 0.985 }}
          key={state.mode}
          transition={{ duration: 0.15, ease: [0.2, 0.82, 0.24, 1] }}
          {...stylex.props(styles.modeSurface)}
        >
          <PopupContent
            state={state}
            presets={presets}
            close={close}
            dispatch={dispatch}
            runAction={runAction}
          />
        </motion.div>
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
      return (
        <LinkSummaryView
          close={close}
          dispatch={dispatch}
          runAction={runAction}
          url={state.sourceText.trim()}
        />
      );
    case "custom":
      return (
        <CustomView
          customInstruction={state.customInstruction}
          dispatch={dispatch}
          runAction={runAction}
        />
      );
    case "processing":
      return (
        <ProcessingView
          close={close}
          label={state.isLinkSummary ? "Summarizing link" : activeDefinition?.label}
          hint={state.isLinkSummary ? "Retrieving the page content…" : undefined}
        />
      );
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
  const renderAction = (action: WritingPreset, index: number) => (
    <button
      aria-label={action.label}
      data-selected={index === selectedIndex}
      data-writing-action
      key={action.id}
      onClick={() => void runAction(action.id)}
      onFocus={() => dispatch({ type: "SELECT", index })}
      role="menuitem"
      type="button"
      {...stylex.props(styles.action, index === selectedIndex && styles.actionSelected)}
    >
      <Icon name={action.icon} size={16} />
      <span {...stylex.props(styles.actionCopy)}>
        <strong {...stylex.props(styles.actionLabel)}>{action.label}</strong>
        <small {...stylex.props(styles.actionDescription)}>{action.description}</small>
      </span>
    </button>
  );
  return (
    <div data-writing-menu {...stylex.props(styles.menu)}>
      <div data-tauri-drag-region {...stylex.props(styles.top)}>
        <span {...stylex.props(styles.heading)}>
          <span {...stylex.props(styles.eyebrow)}>Writing Tools</span>
          <span {...stylex.props(styles.context)}>
            {applicationName ? `Selected text in ${applicationName}` : "Selected text"}
          </span>
        </span>
        <IconButton
          icon={<Icon name="close" size={14} />}
          label="Close Writing Tools"
          onClick={close}
          size="sm"
          variant="ghost"
          xstyle={styles.overlayIconButton}
        />
      </div>
      <div aria-label="Writing actions" role="menu" {...stylex.props(styles.actions)}>
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
      onSubmit={(event) => {
        event.preventDefault();
        void runAction("custom");
      }}
      {...stylex.props(styles.custom)}
    >
      <IconButton
        icon={<Icon name="arrow-left" size={15} />}
        label="Back to writing actions"
        onClick={() => dispatch({ type: "BACK" })}
        size="sm"
        variant="ghost"
        xstyle={styles.overlayIconButton}
      />
      <input
        aria-label="Custom writing instruction"
        autoComplete="off"
        onChange={(event) => dispatch({ type: "SET_CUSTOM", value: event.target.value })}
        placeholder="Describe your change"
        ref={customInputRef}
        required
        spellCheck
        value={customInstruction}
        {...stylex.props(styles.customInput)}
      />
      <Button
        isDisabled={!customInstruction.trim()}
        label="Run instruction"
        size="sm"
        type="submit"
        xstyle={styles.customSubmit}
      >
        ↵
      </Button>
    </form>
  );
}

function OpeningView() {
  return (
    <div aria-live="polite" {...stylex.props(styles.opening)}>
      <Spinner aria-label="Opening Writing Tools" size="sm" />
    </div>
  );
}

function ProcessingView({
  close,
  label,
  hint,
}: {
  readonly close: () => void;
  readonly label: string | undefined;
  readonly hint?: string;
}) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(
      () => setElapsed(Math.floor((Date.now() - started) / 1000)),
      1000,
    );
    return () => window.clearInterval(timer);
  }, []);

  const statusLabel = label ?? "Working";
  const status = elapsed > 0 ? `${statusLabel} · ${elapsed}s` : statusLabel;
  return (
    <div aria-live="polite" {...stylex.props(styles.processing)}>
      <Spinner aria-label={`Running ${label ?? "writing action"}`} size="sm" />
      <span>{status}</span>
      {hint || elapsed >= 4 ? (
        <span {...stylex.props(styles.processingHint)}>
          {hint ?? "Still working. Closing cancels the request."}
        </span>
      ) : null}
      <IconButton
        icon={<Icon name="close" size={14} />}
        label="Cancel"
        onClick={close}
        size="sm"
        variant="ghost"
        xstyle={styles.overlayIconButton}
      />
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
    <div {...stylex.props(styles.result)}>
      <header data-tauri-drag-region {...stylex.props(styles.resultHeader)}>
        <div>
          <span {...stylex.props(styles.resultEyebrow)}>Writing Tools</span>
          <h1 {...stylex.props(styles.resultTitle)}>{label ?? "Result"}</h1>
        </div>
        <IconButton
          icon={<Icon name="close" size={14} />}
          label="Close result"
          onClick={close}
          size="sm"
          variant="ghost"
          xstyle={styles.overlayIconButton}
        />
      </header>
      {source ? (
        <div {...stylex.props(styles.resultSource)}>
          <span>{source.kind === "youtube" ? "YouTube" : "Website"}</span>
          <span title={source.url} {...stylex.props(styles.resultSourceUrl)}>
            {source.url}
          </span>
        </div>
      ) : null}
      <div aria-live="polite" {...stylex.props(styles.resultBody)}>
        <SafeMarkdown>{resultText}</SafeMarkdown>
      </div>
      {copyError ? (
        <p role="alert" {...stylex.props(styles.resultError)}>
          {copyError}
        </p>
      ) : null}
      <footer {...stylex.props(styles.resultFooter)}>
        <Button
          icon={<Icon name={copied ? "check" : "copy"} size={14} />}
          label={copied ? "Copied" : "Copy"}
          onClick={() => {
            setCopyError(null);
            void nativeBridge
              .copyText(resultText)
              .then(() => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1000);
              })
              .catch(() =>
                setCopyError(
                  "The result could not be copied. Select the text and copy it manually.",
                ),
              );
          }}
          size="sm"
          variant="secondary"
        />
        {canReplace ? (
          <Button
            label="Replace"
            onClick={() => {
              void nativeBridge
                .replaceWritingResult(resultText)
                .then(close)
                .catch((error: unknown) => {
                  dispatch(failureForError(error));
                });
            }}
            size="sm"
            variant="primary"
          />
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

function ErrorView({
  activeAction,
  canRetry,
  close,
  dispatch,
  hasContext,
  message,
  runAction,
}: ErrorViewProps) {
  const canShowRetry = canRetry && activeAction !== null;
  return (
    <div aria-live="assertive" {...stylex.props(styles.error)}>
      <Icon name="error" size={20} />
      <div>
        <strong {...stylex.props(styles.errorTitle)}>Writing Tools</strong>
        <span {...stylex.props(styles.errorMessage)}>{message}</span>
      </div>
      <div {...stylex.props(styles.errorActions)}>
        {canShowRetry && activeAction ? (
          <Button
            label="Retry"
            onClick={() => void runAction(activeAction)}
            size="sm"
            variant="secondary"
          />
        ) : null}
        <Button
          label={hasContext ? "Back" : "Close"}
          onClick={hasContext ? () => dispatch({ type: "BACK" }) : close}
          size="sm"
          variant="secondary"
        />
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
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void runAction("summarize");
      }}
      {...stylex.props(styles.summary)}
    >
      <header data-tauri-drag-region {...stylex.props(styles.popupHeader)}>
        <span>Summarize link</span>
        <IconButton
          icon={<Icon name="close" size={14} />}
          label="Close Writing Tools"
          onClick={close}
          size="sm"
          variant="ghost"
          xstyle={styles.overlayIconButton}
        />
      </header>
      <div {...stylex.props(styles.summaryLink)}>
        <Icon name="connection" size={17} />
        <span {...stylex.props(styles.summaryLinkCopy)}>
          <strong {...stylex.props(styles.summaryLinkHost)}>{hostname}</strong>
          <small title={url} {...stylex.props(styles.summaryLinkUrl)}>
            {url}
          </small>
        </span>
      </div>
      <footer {...stylex.props(styles.summaryFooter)}>
        <Button
          label="Other actions"
          onClick={() => dispatch({ type: "BACK" })}
          size="sm"
          variant="secondary"
        />
        <Button label="Summarize" size="sm" type="submit" variant="primary" />
      </footer>
    </form>
  );
}

function prepareWritingRequest(
  state: WritingToolsState,
  preset: WritingPreset,
): WritingRequest | undefined {
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

const overlayBorder = "rgba(255, 255, 255, 0.09)";

const styles = stylex.create({
  stage: {
    display: "flex",
    width: "100%",
    height: "100%",
    padding: "2px",
    alignItems: "flex-end",
    justifyContent: "center",
  },
  popup: {
    position: "relative",
    inset: "auto",
    width: "100%",
    maxWidth: "none",
    maxHeight: "calc(100vh - 4px)",
    margin: "0",
    padding: "0",
    overflow: "hidden",
    color: "var(--kivo-overlay-text)",
    backgroundColor: "var(--kivo-overlay-bg)",
    backdropFilter: "blur(20px) saturate(1.15)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border)",
    borderRadius: "18px",
    boxShadow: "var(--kivo-overlay-shadow)",
  },
  modeSurface: {
    display: "flex",
    flexDirection: "column",
    minHeight: "0",
    maxHeight: "calc(100vh - 4px)",
  },
  menu: {
    maxHeight: "calc(100vh - 4px)",
    overflowY: "auto",
  },
  top: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: "8px",
    paddingBlockStart: "13px",
    paddingInline: "14px",
    paddingBlockEnd: "5px",
    color: "var(--kivo-overlay-text-secondary)",
  },
  heading: {
    display: "grid",
    gap: "1px",
  },
  eyebrow: {
    color: "var(--kivo-overlay-text)",
    fontSize: "12px",
    fontWeight: 700,
    letterSpacing: "0.01em",
  },
  context: {
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "10.5px",
  },
  actions: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    gap: "3px",
    paddingBlock: "2px",
    paddingBlockEnd: "7px",
    paddingInline: "9px",
  },
  action: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    minHeight: "38px",
    paddingBlock: "6px",
    paddingInline: "9px",
    textAlign: "left",
    color: "var(--kivo-overlay-text)",
    backgroundColor: "transparent",
    borderWidth: "0",
    borderRadius: "var(--radius-element, 10px)",
    fontSize: "14px",
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: "120ms",
    ":hover": {
      backgroundColor: "var(--kivo-overlay-hover)",
    },
  },
  actionSelected: {
    backgroundColor: "var(--kivo-overlay-hover)",
    boxShadow: "inset 2px 0 var(--kivo-overlay-text)",
  },
  actionCopy: {
    display: "grid",
    gap: "1px",
    minWidth: "0",
  },
  actionLabel: {
    fontSize: "12.5px",
    fontWeight: 650,
  },
  actionDescription: {
    display: "none",
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "11px",
    lineHeight: "1.25",
  },
  overlayIconButton: {
    color: "var(--kivo-overlay-text-tertiary)",
    flexShrink: 0,
    ":hover": {
      color: "var(--kivo-overlay-text)",
      backgroundColor: "var(--kivo-overlay-hover)",
    },
  },
  custom: {
    display: "grid",
    gridTemplateColumns: "28px 1fr 30px",
    alignItems: "center",
    gap: "4px",
    height: "50px",
    padding: "7px",
  },
  customInput: {
    width: "100%",
    minHeight: "32px",
    paddingInline: "10px",
    color: "var(--kivo-overlay-text)",
    backgroundColor: "transparent",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "transparent",
    borderRadius: "var(--radius-inner, 6px)",
    ":focus": {
      outline: "none",
      borderColor: "var(--kivo-overlay-border-strong)",
    },
    "::placeholder": {
      color: "var(--kivo-overlay-text-tertiary)",
    },
  },
  customSubmit: {
    height: "28px",
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
    borderColor: "transparent",
  },
  opening: {
    display: "grid",
    placeItems: "center",
    height: "64px",
    color: "var(--kivo-overlay-text-secondary)",
  },
  processing: {
    display: "grid",
    gridTemplateColumns: "17px 1fr 26px",
    alignItems: "center",
    gap: "8px",
    minHeight: "50px",
    paddingBlock: "7px",
    paddingInlineStart: "13px",
    paddingInlineEnd: "8px",
    color: "var(--kivo-overlay-text-secondary)",
  },
  processingHint: {
    gridColumn: "2 / 3",
    margin: "0",
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "11px",
  },
  result: {
    display: "flex",
    flexDirection: "column",
    maxHeight: "calc(100vh - 4px)",
  },
  resultHeader: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    flexShrink: 0,
    paddingBlockStart: "15px",
    paddingInlineStart: "16px",
    paddingInlineEnd: "11px",
    paddingBlockEnd: "11px",
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: overlayBorder,
  },
  resultEyebrow: {
    display: "block",
    marginBottom: "2px",
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "10px",
    fontWeight: 700,
    letterSpacing: "0.045em",
    textTransform: "uppercase",
  },
  resultTitle: {
    margin: "0",
    fontSize: "16px",
    fontWeight: 650,
    letterSpacing: "-0.02em",
  },
  resultSource: {
    display: "flex",
    flexShrink: 0,
    gap: "7px",
    minWidth: "0",
    paddingBlockStart: "8px",
    paddingInline: "15px",
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "11px",
  },
  resultBody: {
    minHeight: "0",
    maxHeight: "252px",
    padding: "13px 15px",
    overflow: "auto",
    overflowWrap: "anywhere",
    userSelect: "text",
  },
  resultSourceUrl: {
    minWidth: "0",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    userSelect: "text",
  },
  resultError: {
    margin: "0",
    paddingBlockStart: "6px",
    paddingInline: "15px",
    color: "var(--color-error)",
    fontSize: "12px",
  },
  resultFooter: {
    display: "flex",
    flexShrink: 0,
    justifyContent: "flex-end",
    gap: "7px",
    paddingBlock: "10px",
    paddingInline: "11px",
    borderTopWidth: "1px",
    borderTopStyle: "solid",
    borderTopColor: overlayBorder,
    backgroundColor: "rgba(255, 255, 255, 0.035)",
  },
  error: {
    display: "grid",
    gridTemplateColumns: "24px minmax(0, 1fr)",
    alignItems: "center",
    gap: "10px",
    minHeight: "86px",
    maxHeight: "calc(100vh - 4px)",
    padding: "12px",
    overflowY: "auto",
    color: "var(--color-error)",
  },
  errorTitle: {
    display: "block",
    color: "var(--kivo-overlay-text)",
  },
  errorMessage: {
    display: "block",
    marginTop: "2px",
    color: "var(--kivo-overlay-text-secondary)",
    fontSize: "12px",
  },
  errorActions: {
    display: "flex",
    flexWrap: "wrap",
    gridColumn: "2",
    gap: "6px",
  },
  summary: {
    display: "flex",
    flexDirection: "column",
    maxHeight: "calc(100vh - 4px)",
    overflowY: "auto",
  },
  popupHeader: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    flexShrink: 0,
    height: "34px",
    paddingInlineStart: "12px",
    paddingInlineEnd: "7px",
    color: "var(--kivo-overlay-text-secondary)",
    fontSize: "11.5px",
    fontWeight: 600,
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: overlayBorder,
  },
  summaryLink: {
    display: "flex",
    alignItems: "center",
    gap: "11px",
    margin: "12px",
    padding: "12px",
    color: "var(--kivo-overlay-text-secondary)",
    backgroundColor: "var(--kivo-overlay-selected)",
    borderRadius: "10px",
  },
  summaryLinkCopy: {
    display: "grid",
    gap: "2px",
    minWidth: "0",
  },
  summaryLinkHost: {
    fontSize: "12.5px",
  },
  summaryLinkUrl: {
    minWidth: "0",
    overflow: "hidden",
    color: "var(--kivo-overlay-text-tertiary)",
    fontSize: "10.5px",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  summaryFooter: {
    display: "flex",
    justifyContent: "space-between",
    paddingInline: "12px",
    paddingBlockEnd: "12px",
  },
});
