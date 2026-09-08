import { useEffect, useState } from "react";
import { DeveloperSurfaceMenu } from "./components/DeveloperSurfaceMenu";
import { FlowBar } from "./features/dictation/FlowBar";
import { OnboardingWindow } from "./features/onboarding/OnboardingWindow";
import { SettingsWindow } from "./features/settings/SettingsWindow";
import { WritingToolsPopup } from "./features/writing-tools/WritingToolsPopup";
import { useSystemPreferences } from "./hooks/useSystemPreferences";
import { nativeBridge } from "./platform/native";
import type { AppContext, Platform } from "./types";

function browserPlatform(): Platform {
  return /Windows/i.test(navigator.userAgent) ? "windows" : "macos";
}

export function App() {
  const [context, setContext] = useState<AppContext | null>(null);
  const [bootstrapError, setBootstrapError] = useState(false);
  const platform = context?.platform ?? browserPlatform();
  const { settings, loading, update } = useSystemPreferences(platform);

  useEffect(() => {
    let active = true;
    void nativeBridge.getContext()
      .then((next) => {
        if (active) setContext(next);
      })
      .catch(() => active && setBootstrapError(true));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!context) return;
    document.documentElement.dataset.platform = context.platform;
    document.documentElement.dataset.surface = context.surface;
  }, [context]);

  if (bootstrapError) {
    return <main className="fatal-surface"><div><strong>Kivo couldn’t start.</strong><span>Please reopen the application.</span></div></main>;
  }
  if (!context) return <main aria-label="Loading Kivo" className="bootstrap-surface" />;

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
  }

  return <>{surface}<DeveloperSurfaceMenu current={context.surface} /></>;
}
