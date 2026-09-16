import { useEffect, useState } from "react";
import type { AppSettings, Platform } from "../types";
import { defaultSettings } from "../types";
import { useNativeEvent } from "./useNativeEvent";
import { nativeBridge } from "../platform/native";

export function useSystemPreferences(platform: Platform) {
  const [settings, setSettings] = useState<AppSettings>(() => defaultSettings(platform));
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useNativeEvent<AppSettings>("settings-changed", setSettings);

  useEffect(() => {
    let active = true;
    void nativeBridge
      .getSettings()
      .then((next) => active && setSettings(next))
      .catch(() => active && setError("Settings could not be loaded. Restart Kivo to try again."))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, []);

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

  return { settings, loading, error, update };
}
