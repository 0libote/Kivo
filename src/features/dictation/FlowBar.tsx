import { useCallback, useEffect, useMemo, useReducer } from "react";
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

export function FlowBar({ platform }: { readonly platform: Platform }) {
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

  return (
    <main className="flow-stage" data-platform={platform}>
      <section
        aria-atomic="true"
        aria-label={flowLabel(state.status, state.message)}
        aria-live="polite"
        className="flow-bar"
        data-state={state.status}
        role="status"
      >
        <div className="flow-bar__content">
          {state.status === "idle" ? (
            <span className="flow-bar__idle"><Icon name="microphone" size={13} /></span>
          ) : null}

          {state.status === "listening" ? (
            <>
              <span className="flow-bar__mic"><Icon name="microphone" size={15} /></span>
              {platform === "macos" ? <span aria-hidden="true" className="waveform">
                {levels.map((bar) => (
                  <i key={bar.id} style={{ "--level": bar.value } as React.CSSProperties} />
                ))}
              </span> : <><span aria-hidden="true" className="flow-bar__input-level" style={{ "--level": state.level } as React.CSSProperties}><i /><i /><i /></span><span className="flow-bar__listening">Listening</span></>}
              <button className="flow-bar__stop" aria-label="Finish dictation" onClick={() => void nativeBridge.stopDictation().catch((error: unknown) => dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Dictation could not finish.",
                canRetry: true,
              }))} type="button"><span /></button>
            </>
          ) : null}

          {state.status === "processing" || state.status === "starting" ? (
            <div className="flow-bar__processing"><Spinner label={state.status === "starting" ? "Starting microphone" : "Finishing dictation"} /><span>{state.status === "starting" ? "Starting" : "Finishing"}</span></div>
          ) : null}

          {state.status === "success" ? (
            <span className="flow-bar__success"><Icon name="check" size={17} /></span>
          ) : null}

          {state.status === "error" ? (
            <div className="flow-bar__error">
              <Icon name="error" size={16} />
              <span>{state.message}</span>
              <div className="flow-bar__error-actions">
              <button onClick={() => void nativeBridge.showSurface("settings").catch((error: unknown) => dispatch({
                type: "FAIL",
                message: error instanceof Error ? error.message : "Kivo could not open its window.",
                canRetry: state.canRetry,
              }))} type="button">Open Kivo</button>
              {state.canRetry ? (
                <button
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
              <button aria-label="Dismiss dictation error" onClick={() => {
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
