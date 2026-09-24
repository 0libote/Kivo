import { Button } from "@astryxdesign/core/Button";
import { Spinner } from "@astryxdesign/core/Spinner";
import * as stylex from "@stylexjs/stylex";
import { motion, useReducedMotion } from "motion/react";
import { type Dispatch, useCallback, useEffect, useReducer } from "react";
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

function eventFromSnapshot(snapshot: DictationSnapshot) {
  switch (snapshot.status) {
    case "idle":
      return { type: "IDLE" } as const;
    case "listening":
      return { type: "LISTEN", sessionId: snapshot.sessionId } as const;
    case "starting":
      return { type: "START" } as const;
    case "processing":
      return { type: "PROCESS" } as const;
    case "success":
      return { type: "SUCCEED" } as const;
    case "error":
      return {
        type: "FAIL",
        message: snapshot.message ?? "Dictation couldn’t finish.",
        canRetry: snapshot.canRetry,
      } as const;
    case "hidden":
      return { type: "HIDE" } as const;
  }
}

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

/** Stagger the idle wave loop so neighbouring bars never move in lockstep. */
function waveDelay(index: number): number {
  if (index % 3 === 1) return -0.18;
  if (index % 3 === 2) return -0.36;
  return 0;
}

const barMotion: Record<DictationStatus, Record<string, number>> = {
  hidden: { opacity: 0, y: 5, scale: 0.92, width: 84, height: 40 },
  idle: { opacity: 1, y: 0, scale: 1, width: 38, height: 38 },
  listening: { opacity: 1, y: 0, scale: 1, width: 116, height: 40 },
  starting: { opacity: 1, y: 0, scale: 1, width: 116, height: 40 },
  processing: { opacity: 1, y: 0, scale: 1, width: 116, height: 40 },
  success: { opacity: 1, y: 0, scale: 1, width: 40, height: 40 },
  error: { opacity: 1, y: 0, scale: 1, width: 376, height: 88 },
};

export function FlowBar({ platform }: { readonly platform: Platform }) {
  const reduceMotion = useReducedMotion();
  const [state, dispatch] = useReducer(dictationReducer, {
    ...initialDictationState,
    status: mockInitialStatus(),
    message: mockInitialStatus() === "error" ? "Couldn’t hear that." : null,
    canRetry: mockInitialStatus() === "error",
  });

  const onSnapshot = useCallback(
    (snapshot: DictationSnapshot) => dispatch(eventFromSnapshot(snapshot)),
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
        animate={barMotion[state.status]}
        transition={
          reduceMotion
            ? { duration: 0 }
            : { type: "spring", stiffness: 520, damping: 38, mass: 0.7 }
        }
        {...stylex.props(styles.bar, inert && styles.inert)}
      >
        <div {...stylex.props(styles.content)}>
          {state.status === "idle" ? <IdleContent reduceMotion={reduceMotion} /> : null}

          {state.status === "listening" ? (
            <ListeningContent level={level} reduceMotion={reduceMotion} dispatch={dispatch} />
          ) : null}

          {state.status === "processing" || state.status === "starting" ? (
            <ProcessingContent status={state.status} />
          ) : null}

          {state.status === "success" ? <SuccessContent reduceMotion={reduceMotion} /> : null}

          {state.status === "error" ? (
            <ErrorContent message={state.message} canRetry={state.canRetry} dispatch={dispatch} />
          ) : null}
        </div>
      </motion.section>
    </main>
  );
}

function IdleContent({ reduceMotion }: { readonly reduceMotion: boolean | null }) {
  return (
    <span {...stylex.props(styles.idle)}>
      <motion.span
        animate={
          reduceMotion
            ? { opacity: 0.35, scale: 1 }
            : { opacity: [0.55, 0.18, 0.55], scale: [1, 0.86, 1] }
        }
        transition={
          reduceMotion
            ? { duration: 0 }
            : { duration: 2.4, repeat: Number.POSITIVE_INFINITY, ease: "easeInOut" }
        }
        {...stylex.props(styles.idleRing)}
      />
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

function SuccessContent({ reduceMotion }: { readonly reduceMotion: boolean | null }) {
  return (
    <motion.span
      animate={{ opacity: 1, scale: reduceMotion ? 1 : [0.65, 1] }}
      transition={reduceMotion ? { duration: 0 } : { duration: 0.18, ease: [0.2, 0.9, 0.25, 1.2] }}
      {...stylex.props(styles.success)}
    >
      <Icon name="check" size={17} />
    </motion.span>
  );
}

type FlowDispatch = Dispatch<DictationEvent>;

function ListeningContent({
  level,
  reduceMotion,
  dispatch,
}: {
  readonly level: number;
  readonly reduceMotion: boolean | null;
  readonly dispatch: FlowDispatch;
}) {
  return (
    <>
      <span {...stylex.props(styles.mic)}>
        <motion.span
          animate={
            reduceMotion
              ? { opacity: 0.35, scale: 1 }
              : { opacity: [0.5, 0, 0], scale: [0.7, 1.45, 1.45] }
          }
          transition={
            reduceMotion
              ? { duration: 0 }
              : { duration: 1.4, repeat: Number.POSITIVE_INFINITY, ease: "easeOut" }
          }
          {...stylex.props(styles.micRing)}
        />
        <Icon name="microphone" size={15} />
      </span>
      <span aria-hidden="true" {...stylex.props(styles.waveform)}>
        {BARS.map((bar, index) => (
          <motion.i
            animate={{
              height: Math.max(3, 3 + Math.min(1, level * bar.weight + (bar.odd ? 0.08 : 0)) * 15),
              scaleY: reduceMotion ? 1 : [0.55, 1, 0.55],
            }}
            key={bar.id}
            transition={
              reduceMotion
                ? { duration: 0 }
                : {
                    height: { duration: 0.075, ease: "linear" },
                    scaleY: {
                      duration: 0.72,
                      repeat: Number.POSITIVE_INFINITY,
                      ease: "easeInOut",
                      delay: waveDelay(index),
                    },
                  }
            }
            {...stylex.props(styles.waveBar)}
          />
        ))}
      </span>
      <button
        aria-label="Finish dictation"
        onClick={() =>
          void nativeBridge.stopDictation().catch((error: unknown) =>
            dispatch({
              type: "FAIL",
              message: error instanceof Error ? error.message : "Dictation could not finish.",
              canRetry: true,
            }),
          )
        }
        type="button"
        {...stylex.props(styles.stop)}
      >
        <span {...stylex.props(styles.stopGlyph)} />
      </button>
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
    padding: "2px",
    alignItems: "flex-end",
    justifyContent: "center",
  },
  bar: {
    pointerEvents: "auto",
    overflow: "hidden",
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-overlay-text)",
    backgroundColor: "var(--kivo-overlay-bg)",
    backdropFilter: "blur(20px) saturate(1.2)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border)",
    borderRadius: "20px",
    boxShadow: "var(--kivo-overlay-shadow)",
    transformOrigin: "bottom",
  },
  inert: {
    pointerEvents: "none",
  },
  content: {
    display: "flex",
    height: "100%",
    alignItems: "center",
    justifyContent: "center",
    paddingInline: "12px",
  },
  idle: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    color: "#d4d4d4",
  },
  idleRing: {
    position: "absolute",
    inset: "-7px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.16)",
    borderRadius: "50%",
  },
  mic: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-overlay-text)",
    marginInlineEnd: "9px",
  },
  micRing: {
    position: "absolute",
    inset: "-6px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.12)",
    borderRadius: "50%",
  },
  waveform: {
    display: "flex",
    height: "19px",
    alignItems: "center",
    gap: "2.5px",
    color: "var(--kivo-overlay-text)",
  },
  waveBar: {
    display: "block",
    width: "2px",
    minHeight: "3px",
    maxHeight: "18px",
    borderRadius: "2px",
    backgroundColor: "currentColor",
    transformOrigin: "center",
  },
  stop: {
    display: "grid",
    placeItems: "center",
    width: "28px",
    height: "28px",
    marginInlineStart: "8px",
    padding: "0",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--kivo-overlay-border-strong)",
    borderRadius: "50%",
    backgroundColor: "var(--kivo-overlay-hover)",
    color: "var(--kivo-overlay-text)",
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: "120ms",
    ":hover": {
      backgroundColor: "rgba(255, 255, 255, 0.16)",
    },
  },
  stopGlyph: {
    display: "block",
    width: "8px",
    height: "8px",
    borderRadius: "2px",
    backgroundColor: "currentColor",
  },
  processing: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    color: "#d4d4d4",
    fontSize: "12px",
  },
  success: {
    display: "grid",
    placeItems: "center",
    color: "var(--kivo-signal)",
  },
  error: {
    display: "grid",
    gridTemplateColumns: "16px minmax(0, 1fr)",
    alignItems: "center",
    rowGap: "7px",
    columnGap: "7px",
    paddingBlock: "12px",
    paddingInline: "14px",
    color: "var(--color-error)",
    fontSize: "11.5px",
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
    gap: "8px",
  },
});
