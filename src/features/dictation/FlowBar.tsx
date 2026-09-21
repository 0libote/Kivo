import { useCallback, useEffect, useMemo, useReducer } from "react";
import * as stylex from "@stylexjs/stylex";
import { Icon } from "../../components/Icon";
import { Spinner } from "../../components/Spinner";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge } from "../../platform/native";
import type { DictationSnapshot, Platform } from "../../types";
import { dictationReducer, initialDictationState, type DictationStatus } from "./state";

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

const flowEnter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(7px) scale(.9)" },
  to: { opacity: 1, transform: "none" },
});

const successPop = stylex.keyframes({
  from: { opacity: ".2", transform: "scale(.65)" },
  to: { opacity: 1, transform: "none" },
});

const idlePulse = stylex.keyframes({
  "50%": { opacity: ".35", transform: "scale(.82)" },
});

const listeningRing = stylex.keyframes({
  from: { opacity: ".5", transform: "scale(.7)" },
  to: { opacity: 0, transform: "scale(1.45)" },
});

const wave = stylex.keyframes({
  from: { transform: "scaleY(.55)" },
  to: { transform: "scaleY(1)" },
});

const styles = stylex.create({
  stage: {
    display: "flex",
    alignItems: "flex-end",
    justifyContent: "center",
    width: "100%",
    height: "100%",
    padding: "2px",
    pointerEvents: "none",
    background: "none",
  },
  bar: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    minWidth: "84px",
    height: "40px",
    overflow: "hidden",
    color: "#f8f8f8",
    backgroundColor: "#16171afa",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "#ffffff1a",
    borderRadius: "20px",
    boxShadow: "0 18px 48px #0000004d, inset 0 1px #ffffff0f",
    backdropFilter: "blur(20px) saturate(1.2)",
    transformOrigin: "bottom",
    pointerEvents: "auto",
    transitionProperty: "min-width, opacity, transform",
    transitionDuration: ".17s, .12s, .15s",
    animationName: flowEnter,
    animationDuration: ".17s",
    animationTimingFunction: "cubic-bezier(.2, .84, .26, 1)",
    animationFillMode: "both",
    "@media (forced-colors: active)": { color: "canvastext", backgroundColor: "canvas" },
  },
  hidden: { opacity: 0, pointerEvents: "none", transform: "translateY(5px) scale(.92)" },
  idle: { pointerEvents: "none", opacity: 1, width: "38px", minWidth: "34px", height: "38px" },
  wide: { minWidth: "116px" },
  success: { width: "40px", minWidth: "40px" },
  error: {
    borderRadius: "14px",
    minWidth: "174px",
    maxWidth: "376px",
    height: "auto",
    minHeight: "80px",
  },
  idleRing: {
    "::after": {
      content: '""',
      position: "absolute",
      inset: "7px",
      borderWidth: "1px",
      borderStyle: "solid",
      borderColor: "#ffffff29",
      borderRadius: "50%",
      animationName: idlePulse,
      animationDuration: "2.4s",
      animationTimingFunction: "ease-in-out",
      animationIterationCount: "infinite",
    },
  },
  content: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    paddingInline: "12px",
  },
  mic: {
    position: "relative",
    marginRight: "9px",
    color: "#f4f4f4",
    "::after": {
      content: '""',
      position: "absolute",
      inset: "-6px",
      borderWidth: "1px",
      borderStyle: "solid",
      borderColor: "#ffffff1f",
      borderRadius: "50%",
      animationName: listeningRing,
      animationDuration: "1.4s",
      animationTimingFunction: "ease-out",
      animationIterationCount: "infinite",
    },
  },
  idleIcon: {
    display: "grid",
    placeItems: "center",
    color: "#d4d4d4",
    zIndex: 1,
  },
  waveform: {
    display: "flex",
    alignItems: "center",
    gap: "2.5px",
    height: "19px",
    color: "#f4f4f4",
  },
  waveBar: {
    width: "2px",
    minHeight: "3px",
    maxHeight: "18px",
    height: "calc(3px + var(--level) * 15px)",
    backgroundColor: "currentColor",
    borderRadius: "2px",
    transformOrigin: "center",
    transitionProperty: "height",
    transitionDuration: "75ms",
    transitionTimingFunction: "linear",
    animationName: wave,
    animationDuration: ".72s",
    animationTimingFunction: "ease-in-out",
    animationIterationCount: "infinite",
    animationDirection: "alternate",
  },
  processing: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    fontSize: "12px",
    color: "#d4d4d4",
  },
  successIcon: {
    color: "#90e7b7",
    animationName: successPop,
    animationDuration: ".18s",
    animationTimingFunction: "cubic-bezier(.2, .9, .25, 1.2)",
  },
  errorGrid: {
    display: "grid",
    gridTemplateColumns: "16px minmax(0, 1fr)",
    alignItems: "center",
    gap: "7px",
    fontSize: "11.5px",
    color: "var(--danger)",
  },
  errorMessage: {
    color: "#f4f4f4",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "normal",
    lineHeight: "1.35",
  },
  errorActions: {
    gridColumn: "2",
    display: "flex",
    justifyContent: "flex-end",
    gap: "8px",
  },
  errorButton: {
    flexShrink: 0,
    paddingBlock: "4px",
    paddingInline: "7px",
    color: "#f4f4f4",
    backgroundColor: "#313731",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "#576057",
    borderRadius: "5px",
    cursor: "pointer",
  },
  stopButton: {
    display: "grid",
    placeItems: "center",
    width: "28px",
    height: "28px",
    marginLeft: "8px",
    padding: 0,
    borderRadius: "50%",
    backgroundColor: "#ffffff12",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "#ffffff1a",
    cursor: "pointer",
    ":hover": { backgroundColor: "#ffffff29" },
  },
  stopIcon: {
    width: "8px",
    height: "8px",
    borderRadius: "2px",
    backgroundColor: "currentColor",
  },
});

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
  if (candidate === "starting" || candidate === "idle" || candidate === "processing" || candidate === "success" || candidate === "error" || candidate === "hidden") {
    return candidate;
  }
  return "listening";
}

export function FlowBar({ platform: _platform }: { readonly platform: Platform }) {
  const [state, dispatch] = useReducer(dictationReducer, {
    ...initialDictationState,
    status: mockInitialStatus(),
    message: mockInitialStatus() === "error" ? "Couldn’t hear that." : null,
    canRetry: mockInitialStatus() === "error",
  });

  const onSnapshot = useCallback((snapshot: DictationSnapshot) => dispatch(eventFromSnapshot(snapshot)), []);
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

  const levels = useMemo(
    () => BARS.map((bar) => ({ id: bar.id, value: Math.max(0.18, state.level * bar.weight + (bar.odd ? 0.08 : 0)) })),
    [state.level],
  );

  const stateStyle =
    state.status === "hidden" ? styles.hidden
    : state.status === "idle" ? styles.idle
    : state.status === "starting" ? styles.wide
    : state.status === "processing" ? styles.wide
    : state.status === "success" ? styles.success
    : state.status === "error" ? styles.error
    : null;
  const stage = stylex.props(styles.stage);
  const bar = stylex.props(styles.bar, stateStyle, state.status === "idle" && styles.idleRing);
  const content = stylex.props(styles.content);

  return (
    <main {...stage}>
      <section
        {...bar}
        aria-atomic="true"
        aria-label={flowLabel(state.status, state.message)}
        aria-live="polite"
        className={[bar.className, "flow-bar"].filter(Boolean).join(" ")}
        data-state={state.status}
        role="status"
      >
        <div {...content}>
          {state.status === "idle" ? (
            <span {...stylex.props(styles.idleIcon)}><Icon name="microphone" size={13} /></span>
          ) : null}

          {state.status === "listening" ? (
            <>
              <span {...stylex.props(styles.mic)}><Icon name="microphone" size={15} /></span>
              <span aria-hidden="true" {...stylex.props(styles.waveform)}>
                {levels.map((barData, index) => {
                  const barStyle = stylex.props(styles.waveBar);
                  const delay = (index + 1) % 2 === 0 ? "-180ms" : (index + 1) % 3 === 0 ? "-360ms" : undefined;
                  return (
                    <i
                      className={barStyle.className}
                      key={barData.id}
                      style={{ ...barStyle.style, "--level": barData.value, animationDelay: delay } as React.CSSProperties}
                    />
                  );
                })}
              </span>
              <button {...stylex.props(styles.stopButton)} aria-label="Finish dictation" onClick={() => void nativeBridge.stopDictation().catch((error: unknown) => dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Dictation could not finish.",
                canRetry: true,
              }))} type="button"><span {...stylex.props(styles.stopIcon)} /></button>
            </>
          ) : null}

          {state.status === "processing" || state.status === "starting" ? (
            <div {...stylex.props(styles.processing)}><Spinner label={state.status === "starting" ? "Starting microphone" : "Finishing dictation"} /><span>{state.status === "starting" ? "Starting" : "Finishing"}</span></div>
          ) : null}

          {state.status === "success" ? (
            <span {...stylex.props(styles.successIcon)}><Icon name="check" size={17} /></span>
          ) : null}

          {state.status === "error" ? (
            <div {...stylex.props(styles.errorGrid)}>
              <Icon name="error" size={16} />
              <span {...stylex.props(styles.errorMessage)}>{state.message}</span>
              <div {...stylex.props(styles.errorActions)}>
              <button {...stylex.props(styles.errorButton)} onClick={() => void nativeBridge.showSurface("settings").catch((error: unknown) => dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Kivo could not open its window.",
                canRetry: state.canRetry,
              }))} type="button">Open Kivo</button>
              {state.canRetry ? (
                <button
                  {...stylex.props(styles.errorButton)}
                  onClick={() => {
                    void nativeBridge.retryDictation().catch((error: unknown) => dispatch({
                      type: "FAIL",
                      message: error instanceof Error ? error.message : "Dictation could not restart.",
                      canRetry: true,
                    }));
                  }}
                  type="button"
                >
                  Retry
                </button>
              ) : null}
              <button {...stylex.props(styles.errorButton)} aria-label="Dismiss dictation error" onClick={() => {
                dispatch({ type: "HIDE" });
                void nativeBridge.cancelDictation().catch(() => {
                  // Local state already hid the error; nothing to report.
                });
              }} type="button">Dismiss</button>
              </div>
            </div>
          ) : null}
        </div>
      </section>
    </main>
  );
}

function flowLabel(status: DictationStatus, message: string | null) {
  switch (status) {
    case "idle": return "Dictation ready";
    case "listening": return "Listening";
    case "starting": return "Starting microphone";
    case "processing": return "Finishing dictation";
    case "success": return "Dictation inserted";
    case "error": return message ?? "Dictation error";
    case "hidden": return "";
  }
}
