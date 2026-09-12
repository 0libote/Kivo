import { useEffect, useState } from "react";
import { DeveloperSurfaceMenu } from "./components/DeveloperSurfaceMenu";
import { FlowBar } from "./features/dictation/FlowBar";
import { OnboardingWindow } from "./features/onboarding/OnboardingWindow";
import { SettingsWindow } from "./features/settings/SettingsWindow";
import { WritingToolsPopup } from "./features/writing-tools/WritingToolsPopup";
import { useSystemPreferences } from "./hooks/useSystemPreferences";
import { nativeBridge, initialAppContext } from "./platform/native";
import type { AppContext } from "./types";

export function App() {
  // ponytail: render the window-label surface immediately; context hydrates async.
  const [context, setContext] = useState<AppContext>(() => initialAppContext());
  const platform = context.platform;
  const { settings, loading, update } = useSystemPreferences(platform);

  useEffect(() => {
    let active = true;
    void nativeBridge.getContext()
      .then((next) => {
        if (active) setContext(next);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.platform = context.platform;
    document.documentElement.dataset.surface = context.surface;
  }, [context]);

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
