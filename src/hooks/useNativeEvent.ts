import { useEffect } from "react";
import { nativeBridge } from "../platform/native";

type EventName = Parameters<typeof nativeBridge.on>[0];

export function useNativeEvent<T>(event: EventName, handler: (payload: T) => void) {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void nativeBridge
      .on(event, (payload) => handler(payload as T))
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event, handler]);
}
