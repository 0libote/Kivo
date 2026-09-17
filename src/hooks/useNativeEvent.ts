import { useEffect, useRef } from "react";
import { nativeBridge } from "../platform/native";

type EventName = Parameters<typeof nativeBridge.on>[0];

export function useNativeEvent<T>(event: EventName, handler: (payload: T) => void) {
  const latest = useRef(handler);
  useEffect(() => { latest.current = handler; }, [handler]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void nativeBridge
      .on(event, (payload) => latest.current(payload as T))
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      }).catch(() => {
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
