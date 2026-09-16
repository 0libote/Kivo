import { useCallback, useEffect, useState } from "react";
import type { AppSettings, Platform } from "../types";
import { defaultSettings } from "../types";
import { useNativeEvent } from "./useNativeEvent";
import { nativeBridge } from "../platform/native";

export function useSystemPreferences(platform: Platform) {
  const [settings, setSettings] = useState<AppSettings>(() => defaultSettings(platform));
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useNativeEvent<AppSettings>("settings-changed", setSettings);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    return nativeBridge
      .getSettings()
      .then((next) => setSettings(next))
      .catch(() => setError("Settings could not be loaded. Restart Kivo to try again."))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    // load() records failures in state and never rejects.
    void load();
  }, [load]);

  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
  }, [settings.theme]);

  async function update(patch: Partial<AppSettings>) {
    setSettings((current) => ({ ...current, ...patch }));
    try {
      const persisted = await nativeBridge.updateSettings(patch);
      setSettings(persisted);
      return persisted;
    } catch (error) {
      // Refresh from the native side so a failed write never leaves the UI
      // showing state that was not persisted. A failed refresh must not mask
      // the original error, so it is intentionally swallowed here.
      try {
        const persisted = await nativeBridge.getSettings();
        setSettings(persisted);
      } catch {
        // Keep the optimistic state; the caller still sees the real failure.
      }
      throw error;
    }
  }

  return { settings, loading, error, update, retry: load };
}
