import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { Fragment, type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { Icon, type IconName } from "../../components/Icon";
import { useNativeEvent } from "../../hooks/useNativeEvent";
import { nativeBridge, type UpdateResult } from "../../platform/native";
import {
  type ApiKeyStatus,
  type AppContext,
  type AppSettings,
  type MicrophoneDevice,
  NativeError,
  type SpeechLanguage,
} from "../../types";
import { AiSection } from "./AiSection";
import { DictationSection } from "./DictationSection";
import { HomeSection } from "./HomeSection";
import { SystemSettingsSection } from "./SystemSettingsSection";
import type { SaveSettings } from "./settings-layout";
import { styles } from "./settings-styles";
import { VocabularySection, WritingSection } from "./WritingSection";

type SettingsSection = "home" | "dictation" | "writing" | "vocabulary" | "ai" | "settings";

interface SettingsWindowProps {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly loading: boolean;
  readonly updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
}

const SECTIONS: Array<{ id: SettingsSection; label: string; icon: IconName }> = [
  { id: "home", label: "Home", icon: "home" },
  { id: "dictation", label: "Dictation", icon: "microphone" },
  { id: "writing", label: "Writing Tools", icon: "pencil" },
  { id: "vocabulary", label: "Custom words", icon: "spellcheck" },
  { id: "ai", label: "AI", icon: "connection" },
  { id: "settings", label: "Settings", icon: "settings" },
];

const NAV_GROUP_LABELS: Partial<Record<SettingsSection, string>> = {
  home: "Workspace",
  dictation: "Tools",
  settings: "System",
};

export function SettingsWindow({
  context,
  settings,
  loading,
  updateSettings,
}: SettingsWindowProps) {
  const [section, setSection] = useState<SettingsSection>("home");
  const [microphones, setMicrophones] = useState<MicrophoneDevice[]>([]);
  const [languages, setLanguages] = useState<SpeechLanguage[]>([]);
  const [apiStatus, setApiStatus] = useState<ApiKeyStatus>({
    configured: false,
    connection: "untested",
  });
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateResult | null>(null);

  useNativeEvent("show-about", () => setSection("settings"));

  useEffect(() => {
    let active = true;
    // allSettled never rejects; each list degrades independently below.
    void Promise.allSettled([
      nativeBridge.listMicrophones(),
      nativeBridge.listSpeechLanguages(),
      nativeBridge.getApiKeyStatus(),
    ]).then(([nextMicrophones, nextLanguages, nextApiStatus]) => {
      if (!active) return;
      if (nextMicrophones.status === "fulfilled") setMicrophones(nextMicrophones.value);
      if (nextLanguages.status === "fulfilled") setLanguages(nextLanguages.value);
      if (nextApiStatus.status === "fulfilled") setApiStatus(nextApiStatus.value);
      if (
        [nextMicrophones, nextLanguages, nextApiStatus].some(
          (result) => result.status === "rejected",
        )
      )
        setNotice("Some settings could not be loaded. Reopen Settings to try again.");
    });
    return () => {
      active = false;
    };
  }, []);

  const save = useCallback<SaveSettings>(
    async (patch) => {
      setNotice(null);
      try {
        await updateSettings(patch);
      } catch (error) {
        setNotice(error instanceof NativeError ? error.message : "The setting couldn’t be saved.");
      }
    },
    [updateSettings],
  );

  return (
    <main data-loading={loading} data-platform={context.platform} {...stylex.props(styles.shell)}>
      <aside aria-label="Settings sections" {...stylex.props(styles.sidebar)}>
        <div {...stylex.props(styles.brand)}>
          <span {...stylex.props(styles.brandMark)}>
            <Icon name="audio" size={17} />
          </span>
          <strong {...stylex.props(styles.brandName)}>Kivo</strong>
        </div>
        <nav {...stylex.props(styles.nav)}>
          {SECTIONS.map((item) => (
            <Fragment key={item.id}>
              {NAV_GROUP_LABELS[item.id] ? (
                <span {...stylex.props(styles.groupLabel)}>{NAV_GROUP_LABELS[item.id]}</span>
              ) : null}
              <button
                aria-current={section === item.id ? "page" : undefined}
                onClick={() => setSection(item.id)}
                type="button"
                {...stylex.props(styles.navButton, section === item.id && styles.navButtonCurrent)}
              >
                <Icon name={item.icon} size={16} />
                <span>{item.label}</span>
              </button>
            </Fragment>
          ))}
        </nav>
        <p {...stylex.props(styles.sidebarStatus)}>
          <span
            aria-hidden="true"
            {...stylex.props(styles.statusDot, context.paused && styles.statusDotPaused)}
          />
          {context.paused ? "Paused" : "Ready"}
          <span {...stylex.props(styles.version)}>{context.version}</span>
        </p>
      </aside>
      <SettingsMain key={section}>
        {section === "home" ? (
          <HomeSection
            context={context}
            settings={settings}
            onWriting={() => setSection("writing")}
            onDictation={() => setSection("dictation")}
            onVocabulary={() => setSection("vocabulary")}
          />
        ) : (
          <SectionContent
            apiKey={apiKey}
            apiStatus={apiStatus}
            busy={busy}
            context={context}
            languages={languages}
            microphones={microphones}
            onVocabulary={() => setSection("vocabulary")}
            section={section}
            setApiKey={setApiKey}
            setApiStatus={setApiStatus}
            setBusy={setBusy}
            setNotice={setNotice}
            setUpdateResult={setUpdateResult}
            settings={settings}
            save={save}
            updateResult={updateResult}
          />
        )}
        {notice ? (
          <div aria-live="polite" data-testid="settings-notice" {...stylex.props(styles.notice)}>
            {notice}
            <Button
              icon={<Icon name="close" size={12} />}
              isIconOnly
              label="Dismiss message"
              onClick={() => setNotice(null)}
              size="sm"
              variant="ghost"
            />
          </div>
        ) : null}
      </SettingsMain>
    </main>
  );
}

function SettingsMain({ children }: { readonly children: ReactNode }) {
  const mainRef = useRef<HTMLDivElement>(null);
  // Move focus to the new section on navigation only (mount), so keyboard and
  // screen-reader users land on the fresh heading. Runs once per section
  // because the parent remounts this component via key={section}.
  useEffect(() => {
    mainRef.current?.focus({ preventScroll: true });
  }, []);
  return (
    <div data-testid="settings-main" ref={mainRef} tabIndex={-1} {...stylex.props(styles.main)}>
      {children}
    </div>
  );
}

interface SectionContentProps {
  readonly apiKey: string;
  readonly apiStatus: ApiKeyStatus;
  readonly busy: string | null;
  readonly context: AppContext;
  readonly languages: SpeechLanguage[];
  readonly microphones: MicrophoneDevice[];
  readonly onVocabulary: () => void;
  readonly section: SettingsSection;
  readonly setApiKey: (value: string) => void;
  readonly setApiStatus: (value: ApiKeyStatus | ((current: ApiKeyStatus) => ApiKeyStatus)) => void;
  readonly setBusy: (value: string | null) => void;
  readonly setNotice: (value: string | null) => void;
  readonly setUpdateResult: (value: UpdateResult) => void;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly updateResult: UpdateResult | null;
}

function SectionContent(props: SectionContentProps) {
  switch (props.section) {
    case "home":
      return null;
    case "settings":
      return (
        <SystemSettingsSection
          busy={props.busy}
          context={props.context}
          setBusy={props.setBusy}
          setNotice={props.setNotice}
          setUpdateResult={props.setUpdateResult}
          settings={props.settings}
          save={props.save}
          updateResult={props.updateResult}
        />
      );
    case "dictation":
      return (
        <DictationSection
          context={props.context}
          languages={props.languages}
          microphones={props.microphones}
          settings={props.settings}
          save={props.save}
          onVocabulary={props.onVocabulary}
        />
      );
    case "writing":
      return (
        <WritingSection
          context={props.context}
          settings={props.settings}
          save={props.save}
          onVocabulary={props.onVocabulary}
        />
      );
    case "vocabulary":
      return (
        <VocabularySection
          settings={props.settings}
          save={props.save}
          setNotice={props.setNotice}
        />
      );
    case "ai":
      return (
        <AiSection
          apiKey={props.apiKey}
          apiStatus={props.apiStatus}
          busy={props.busy}
          setApiKey={props.setApiKey}
          setApiStatus={props.setApiStatus}
          setBusy={props.setBusy}
          setNotice={props.setNotice}
          settings={props.settings}
          save={props.save}
        />
      );
  }
}
