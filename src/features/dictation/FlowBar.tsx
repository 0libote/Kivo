import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Spinner } from "@astryxdesign/core/Spinner";
import * as stylex from "@stylexjs/stylex";
import { motion, useReducedMotion } from "motion/react";
import { type Dispatch, useCallback, useEffect, useReducer, useRef } from "react";
import { Icon } from "../../components/Icon";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { DictationSnapshot, Platform } from "../../types";
import {
  type DictationEvent,
  type DictationStatus,
  dictationReducer,
  initialDictationState,
} from "./state";

const BARS: Array<{ readonly id: string; readonly weight: number; readonly odd: boolean }> = [
  { id: "bar-0", weight: 0.34, odd: false },
  { id: "bar-1", weight: 0.62, odd: true },
  { id: "bar-2", weight: 0.84, odd: false },
  { id: "bar-3", weight: 0.52, odd: true },
  { id: "bar-4", weight: 1, odd: false },
  { id: "bar-5", weight: 0.69, odd: true },
  { id: "bar-6", weight: 0.88, odd: false },
  { id: "bar-7", weight: 0.58, odd: true },
  { id: "bar-8", weight: 0.3, odd: false },
];

function mockInitialStatus(): DictationStatus {
  if (nativeBridge.isNative) return "hidden";
  const candidate = new URLSearchParams(window.location.search).get("state");
  if (
    candidate === "starting" ||
    candidate === "idle" ||
    candidate === "processing" ||
    candidate === "success" ||
    candidate === "error" ||
    candidate === "hidden"
  ) {
    return candidate;
  }
  return "listening";
}

/** Calm overlay contract: the native shell owns the window size (it animates
    the frame with a short ease-out in `size_flow_bar`), so React must not
    animate width/height/scale here. Competing size springs were the wobble:
    the pill overshot while the frame stepped, and the `wait`-sequenced
    crossfade left an empty pill mid-transition. React only fades content. */

export function FlowBar({ platform }: { readonly platform: Platform }) {
  const reduceMotion = useReducedMotion();
  const [state, dispatch] = useReducer(dictationReducer, {
    ...initialDictationState,
    status: mockInitialStatus(),
    message: mockInitialStatus() === "error" ? "Couldn’t hear that." : null,
    canRetry: mockInitialStatus() === "error",
  });

  const onSnapshot = useCallback(
    (snapshot: DictationSnapshot) => dispatch({ type: "SNAPSHOT", snapshot }),
    [],
  );
  const onLevel = useCallback((level: number) => dispatch({ type: "LEVEL", level }), []);
  useNativeEvent<DictationSnapshot>("dictation-state", onSnapshot);
  useNativeEvent<number>("dictation-level", onLevel);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape" || state.status === "hidden" || state.status === "idle") return;
      event.preventDefault();
      dispatch({ type: "CANCEL" });
      void nativeBridge.cancelDictation().catch(() => {
        // Local state already cancelled; a backend failure here is not
        // user-actionable (the session is gone from the UI's perspective).
      });
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [state.status]);

  useEffect(() => {
    if (nativeBridge.isNative || state.status !== "success") return;
    const timer = window.setTimeout(() => dispatch({ type: "HIDE" }), 620);
    return () => window.clearTimeout(timer);
  }, [state.status]);

  const level = state.level;
  const inert = state.status === "hidden" || state.status === "idle";

  return (
    <main data-platform={platform} {...stylex.props(styles.stage)}>
      <motion.section
        aria-atomic="true"
        aria-label={flowLabel(state.status, state.message)}
        aria-live="polite"
        data-state={state.status}
        data-testid="flow-bar"
        initial={false}
        animate={{ opacity: state.status === "hidden" ? 0 : 1 }}
        transition={reduceMotion ? { duration: 0 } : { duration: 0.15, ease: "easeOut" }}
        {...stylex.props(styles.bar, inert && styles.inert)}
      >
        {/* Keyed fade-in with no exit: the old content swaps instantly and the
            new content fades in. No `wait` gap (empty pill flash), no `sync`
            overlap (double rows briefly widening the pill). */}
        <motion.div
          key={state.status === "starting" ? "processing" : state.status}
          initial={reduceMotion ? false : { opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={reduceMotion ? { duration: 0 } : { duration: 0.12, ease: "easeOut" }}
          {...stylex.props(styles.content)}
        >
          {state.status === "idle" ? <IdleContent /> : null}

          {state.status === "listening" ? (
            <ListeningContent level={level} dispatch={dispatch} />
          ) : null}

          {state.status === "processing" || state.status === "starting" ? (
            <ProcessingContent status={state.status} />
          ) : null}

          {state.status === "success" ? <SuccessContent /> : null}

          {state.status === "error" ? (
            <ErrorContent message={state.message} canRetry={state.canRetry} dispatch={dispatch} />
          ) : null}
        </motion.div>
      </motion.section>
    </main>
  );
}

function IdleContent() {
  return (
    <span {...stylex.props(styles.idle)}>
      <span {...stylex.props(styles.idleRing)} />
      <Icon name="microphone" size={13} />
    </span>
  );
}

function ProcessingContent({ status }: { readonly status: "processing" | "starting" }) {
  return (
    <div {...stylex.props(styles.processing)}>
      <Spinner
        aria-label={status === "starting" ? "Starting microphone" : "Finishing dictation"}
        shade="onMedia"
        size="sm"
      />
      <span>{status === "starting" ? "Starting" : "Finishing"}</span>
    </div>
  );
}

function SuccessContent() {
  return (
    <span {...stylex.props(styles.success)}>
      <Icon name="check" size={17} />
    </span>
  );
}

type FlowDispatch = Dispatch<DictationEvent>;

function ListeningContent({
  level,
  dispatch,
}: {
  readonly level: number;
  readonly dispatch: FlowDispatch;
}) {
  const stopping = useRef(false);
  return (
    <>
      <span {...stylex.props(styles.mic)}>
        <span {...stylex.props(styles.micRing)} />
        <Icon name="microphone" size={15} />
      </span>
      <span aria-hidden="true" {...stylex.props(styles.waveform)}>
        {BARS.map((bar) => (
          <i
            key={bar.id}
            style={{
              height: Math.max(3, 3 + Math.min(1, level * bar.weight + (bar.odd ? 0.08 : 0)) * 15),
            }}
            {...stylex.props(styles.waveBar)}
          />
        ))}
      </span>
      <IconButton
        label="Finish dictation"
        icon={<span {...stylex.props(styles.stopGlyph)} />}
        size="sm"
        variant="ghost"
        onClick={() => {
          if (stopping.current) return;
          stopping.current = true;
          // Native snapshots must render immediately. clickAction wraps its
          // updates in a React transition and can defer the processing phase.
          void nativeBridge
            .stopDictation()
            .catch((error: unknown) =>
              dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Dictation could not finish.",
                canRetry: true,
              }),
            )
            .finally(() => {
              stopping.current = false;
            });
        }}
        xstyle={styles.stop}
      />
    </>
  );
}

function ErrorContent({
  message,
  canRetry,
  dispatch,
}: {
  readonly message: string | null;
  readonly canRetry: boolean;
  readonly dispatch: FlowDispatch;
}) {
  return (
    <div {...stylex.props(styles.error)}>
      <Icon name="error" size={16} />
      <span {...stylex.props(styles.errorMessage)}>{message}</span>
      <div {...stylex.props(styles.errorActions)}>
        <Button
          label="Open Kivo"
          onClick={() =>
            void nativeBridge.showSurface("settings").catch((error: unknown) =>
              dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Kivo could not open its window.",
                canRetry: canRetry,
              }),
            )
          }
          size="sm"
          variant="secondary"
        />
        {canRetry ? (
          <Button
            label="Retry"
            onClick={() => {
              void nativeBridge.retryDictation().catch((error: unknown) =>
                dispatch({
                  type: "FAIL",
                  message: error instanceof Error ? error.message : "Dictation could not restart.",
                  canRetry: true,
                }),
              );
            }}
            size="sm"
            variant="primary"
          />
        ) : null}
        <Button
          label="Dismiss dictation error"
          onClick={() => {
            dispatch({ type: "HIDE" });
            void nativeBridge.cancelDictation().catch(() => {
              // Local state already hid the error; nothing to report.
            });
          }}
          size="sm"
          variant="secondary"
        >
          Dismiss
        </Button>
      </div>
    </div>
  );
}

function flowLabel(status: DictationStatus, message: string | null) {
  switch (status) {
    case "idle":
      return "Dictation ready";
    case "listening":
      return "Listening";
    case "starting":
      return "Starting microphone";
    case "processing":
      return "Finishing dictation";
    case "success":
      return "Dictation inserted";
    case "error":
      return message ?? "Dictation error";
    case "hidden":
      return "";
  }
}

const styles = stylex.create({
  stage: {
    pointerEvents: "none",
    display: "flex",
    width: "100%",
    height: "100%",
    padding: "var(--spacing-0-5)",
    alignItems: "flex-end",
    justifyContent: "center",
  },
  bar: {
    pointerEvents: "auto",
    overflow: "hidden",
    display: "grid",
    placeItems: "center",
    width: "100%",
    height: "100%",
    color: "var(--kivo-overlay-text)",
    backgroundColor: "var(--kivo-overlay-bg)",
    backdropFilter: "blur(var(--spacing-5)) saturate(1.2)",
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border)",
    borderRadius: "var(--spacing-5)",
    boxShadow: "var(--kivo-overlay-shadow)",
  },
  inert: {
    pointerEvents: "none",
  },
  content: {
    display: "flex",
    height: "100%",
    alignItems: "center",
    justifyContent: "center",
    paddingInline: "var(--spacing-3)",
  },
  idle: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-overlay-text-secondary)",
  },
  idleRing: {
    position: "absolute",
    inset: "calc(var(--spacing-7) / -4)",
    opacity: 0.35,
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border-strong)",
    borderRadius: "var(--radius-full)",
  },
  mic: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-overlay-text)",
    marginInlineEnd: "calc(var(--spacing-3) * 0.75)",
  },
  micRing: {
    position: "absolute",
    inset: "calc(var(--spacing-1-5) * -1)",
    opacity: 0.35,
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-selected)",
    borderRadius: "var(--radius-full)",
  },
  waveform: {
    display: "flex",
    height: "calc(var(--spacing-5) - var(--border-width))",
    alignItems: "center",
    gap: "calc(var(--spacing-5) / 8)",
    color: "var(--kivo-overlay-text)",
  },
  waveBar: {
    display: "block",
    width: "var(--spacing-0-5)",
    minHeight: "calc(var(--spacing-1) * 0.75)",
    maxHeight: "calc(var(--spacing-9) / 2)",
    borderRadius: "var(--spacing-0-5)",
    backgroundColor: "currentColor",
    transformOrigin: "center",
  },
  stop: {
    display: "grid",
    placeItems: "center",
    width: "var(--size-element-sm)",
    height: "var(--size-element-sm)",
    marginInlineStart: "var(--spacing-2)",
    padding: "0",
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border-strong)",
    borderRadius: "var(--radius-full)",
    backgroundColor: "var(--kivo-overlay-hover)",
    color: "var(--kivo-overlay-text)",
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: "var(--duration-fast)",
    ":hover": {
      backgroundColor: "var(--kivo-overlay-border-strong)",
    },
  },
  stopGlyph: {
    display: "block",
    width: "var(--spacing-2)",
    height: "var(--spacing-2)",
    borderRadius: "var(--spacing-0-5)",
    backgroundColor: "currentColor",
  },
  processing: {
    display: "flex",
    alignItems: "center",
    gap: "var(--spacing-2)",
    color: "var(--kivo-overlay-text-secondary)",
    fontSize: "var(--font-size-xs)",
  },
  success: {
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-signal)",
  },
  error: {
    display: "grid",
    gridTemplateColumns: "var(--spacing-4) minmax(0, 1fr)",
    alignItems: "center",
    rowGap: "calc(var(--spacing-7) / 4)",
    columnGap: "calc(var(--spacing-7) / 4)",
    paddingBlock: "var(--spacing-3)",
    paddingInline: "calc(var(--spacing-7) / 2)",
    color: "var(--color-error)",
    fontSize: "var(--font-size-xs)",
  },
  errorMessage: {
    color: "var(--kivo-overlay-text)",
    lineHeight: "1.35",
  },
  errorActions: {
    display: "flex",
    gridColumn: "2",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "var(--spacing-2)",
  },
});
