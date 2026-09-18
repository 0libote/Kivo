import { useEffect, useState } from "react";
import { DeveloperSurfaceMenu } from "./components/DeveloperSurfaceMenu";
import { FlowBar } from "./features/dictation/FlowBar";
import { GalleryWindow } from "./features/gallery/GalleryWindow";
import { OnboardingWindow } from "./features/onboarding/OnboardingWindow";
import { SettingsWindow } from "./features/settings/SettingsWindow";
import { WritingToolsPopup } from "./features/writing-tools/WritingToolsPopup";
import { useNativeEvent } from "./hooks/useNativeEvent";
import { useSystemPreferences } from "./hooks/useSystemPreferences";
import { nativeBridge, initialAppContext } from "./platform/native";
import type { AppContext } from "./types";

export function App() {
  // Render the window-label surface immediately; context hydrates async.
  const [context, setContext] = useState<AppContext>(() => initialAppContext());
  const [contextError, setContextError] = useState<string | null>(null);
  useNativeEvent<boolean>("pause-changed", paused => setContext(current => ({ ...current, paused })));
  const platform = context.platform;
  const { settings, loading, error, update, retry } = useSystemPreferences(platform);

  useEffect(() => {
    let active = true;
    void nativeBridge.getContext()
      .then((next) => {
        if (active) {
          setContext(next);
          setContextError(null);
        }
      })
      .catch(() => {
        // Keep the synchronous surface guess so the window still paints, but
        // say so: the version/paused state may be stale.
        if (active) setContextError("Kivo could not reach its background service. Some information may be out of date.");
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.platform = context.platform;
    document.documentElement.dataset.surface = context.surface;
  }, [context]);

  if (error) {
    return (
      <main className="fatal-surface">
        <div>
          <p role="alert">{error}</p>
          <button onClick={() => void retry()} type="button">Try again</button>
        </div>
      </main>
    );
  }

  // The background-service notice only fits the large windows; the compact
  // overlays (flow-bar, writing-tools) have no room for a banner.
  const showServiceNotice = contextError !== null && (context.surface === "settings" || context.surface === "onboarding");

  let surface: React.ReactNode;
  switch (context.surface) {
    case "flow-bar":
      surface = <FlowBar platform={context.platform} />;
      break;
    case "writing-tools":
      surface = <WritingToolsPopup platform={context.platform} settings={settings} />;
      break;
    case "onboarding":
      surface = <OnboardingWindow context={context} settings={settings} updateSettings={update} />;
      break;
    case "settings":
      surface = <SettingsWindow context={context} loading={loading} settings={settings} updateSettings={update} />;
      break;
    case "gallery":
      surface = <GalleryWindow context={context} settings={settings} />;
      break;
  }

  return (
    <>
      {showServiceNotice ? (
        <div className="service-notice" role="status">
          <span>{contextError}</span>
          <button className="text-link" onClick={() => setContextError(null)} type="button">Dismiss</button>
        </div>
      ) : null}
      {surface}
      <DeveloperSurfaceMenu current={context.surface} />
    </>
  );
}
