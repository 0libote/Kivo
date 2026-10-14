import { useEffect, useRef } from "react";
import { isNativeEventPayload } from "../platform/bridge";
import { nativeBridge } from "../platform/native";

type EventName = Parameters<typeof nativeBridge.on>[0];

export function useNativeEvent<T>(event: EventName, handler: (payload: T) => void) {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  }, [handler]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void nativeBridge
      .on(event, (payload) => {
        // The bridge delivers unknown JSON: narrow before the typed handler
        // so a backend shape change degrades to a warning, not a crash on a
        // mistyped payload deep in a subscriber.
        if (isNativeEventPayload(event, payload)) latest.current(payload as T);
        else console.warn(`Kivo ignored an invalid payload for event: ${event}`);
      })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => {
        // A closed native surface may reject registration; warn so a
        // silently-stale subscription (settings, dictation level) is visible
        // in diagnostics instead of failing without a trace.
        console.warn(`Kivo event subscription failed: ${event}`);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event]);
}
