import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { SegmentedControl } from "../../components/SegmentedControl";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";
import { Switch } from "../../components/Switch";
import {
  type AppContext,
  type AppSettings,
  type MicrophoneDevice,
  type SpeechLanguage,
  type VozModelStatus,
} from "../../types";
import { DictationCleanupModel } from "./DictationCleanupModel";
import { LocalSpeechModels } from "./LocalSpeechModels";
import { type SaveSettings, SettingRow, SettingsContent, SettingsGroup } from "./settings-layout";
import { styles } from "./settings-styles";
import { VozSetup } from "./VozSetup";
import { VocabularySummaryRow } from "./WritingSection";

const VOZ_LANGUAGES: SpeechLanguage[] = [
  "bg",
  "cs",
  "da",
  "de",
  "el",
  "en",
  "es",
  "et",
  "fi",
  "fr",
  "hr",
  "hu",
  "it",
  "lt",
  "lv",
  "mt",
  "nl",
  "pl",
  "pt",
  "ro",
  "ru",
  "sk",
  "sl",
  "sv",
  "uk",
].map((code) => ({
  code,
  name: new Intl.DisplayNames(["en"], { type: "language" }).of(code) ?? code,
  installed: true,
  downloadable: false,
}));
VOZ_LANGUAGES.unshift({
  code: "auto",
  name: "Automatic (on-device detection)",
  installed: true,
  downloadable: false,
});

const VOZ_LANGUAGE_CODES = new Set(VOZ_LANGUAGES.map(({ code }) => code));

export function DictationSection({
  context,
  languages,
  microphones,
  settings,
  save,
  onVocabulary,
}: {
  readonly context: AppContext;
  readonly languages: SpeechLanguage[];
  readonly microphones: MicrophoneDevice[];
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly onVocabulary: () => void;
}) {
  const [vozStatus, setVozStatus] = useState<VozModelStatus>({
    supported: false,
    downloaded: false,
    phase: "unsupported",
    progress: null,
    runtime: "Checking availability",
    error: null,
  });
  const selectedLanguage = settings.dictationLanguage.split(/[-_]/)[0].toLowerCase();
  const vozLanguageSupported =
    settings.dictationLanguage === "auto" || VOZ_LANGUAGE_CODES.has(selectedLanguage);
  const languageOptions = settings.speechEngine === "voz" ? VOZ_LANGUAGES : languages;
  const languageDescription = dictationLanguageDescription(settings.speechEngine, context.platform);
  const engineDescription =
    settings.speechEngine === "local" || settings.speechEngine === "voz"
      ? "On-device: audio is transcribed by a model on this computer and never sent anywhere."
      : "System: uses the operating-system speech engine, which may use the network on some systems.";
  return (
    <SettingsContent
      title="Dictation"
      subtitle="Hold your shortcut, speak, then release — or tap to start and tap again to stop."
    >
      <SettingsGroup header="Recognition">
        <SettingRow label="Transcription engine" description={engineDescription} stacked>
          <SegmentedControl
            ariaLabel="Transcription engine"
            onChange={(engine) =>
              void save({ speechEngine: engine as AppSettings["speechEngine"] })
            }
            options={[
              { label: "System", value: "system" },
              { label: "On-device", value: "local" },
              {
                label: "Voz · Recommended",
                value: "voz",
                disabled: !vozStatus.supported || !vozStatus.downloaded || !vozLanguageSupported,
              },
            ]}
            value={settings.speechEngine}
          />
        </SettingRow>
        {settings.speechEngine === "local" ? (
          <SettingRow label="On-device model" stacked>
            <LocalSpeechModels save={save} settings={settings} />
          </SettingRow>
        ) : null}
        <SettingRow label="Voz on-device model" stacked>
          <VozSetup context={context} onStatus={setVozStatus} />
          {!vozLanguageSupported && vozStatus.supported ? (
            <span {...stylex.props(styles.empty)}>
              Select one of Voz’s supported languages to enable Voz as the transcription engine.
            </span>
          ) : null}
        </SettingRow>
        <SettingRow label="Language" description={languageDescription}>
          {languageOptions.length === 0 ? (
            <span {...stylex.props(styles.empty)}>
              No languages found. Reopen Settings to try again.
            </span>
          ) : (
            <select
              aria-label="Dictation language"
              onChange={(event) => void save({ dictationLanguage: event.target.value })}
              value={
                languageOptions.some((language) => language.code === settings.dictationLanguage)
                  ? settings.dictationLanguage
                  : languageOptions[0].code
              }
              {...stylex.props(styles.field)}
            >
              {languageOptions.map((language) => (
                <option key={language.code} value={language.code}>
                  {languageName(language)}
                </option>
              ))}
            </select>
          )}
        </SettingRow>
      </SettingsGroup>
      <SettingsGroup>
        <SettingRow label="Shortcut">
          <ShortcutRecorder
            label="Dictation shortcut"
            onChange={(dictationShortcut) => save({ dictationShortcut })}
            platform={context.platform}
            value={settings.dictationShortcut}
          />
        </SettingRow>
        <SettingRow
          label="Tap to dictate"
          description="A quick press starts listening; press again to finish."
        >
          <Switch
            checked={settings.dictationTapEnabled}
            label="Tap to dictate"
            onChange={(value) => void save({ dictationTapEnabled: value })}
          />
        </SettingRow>
        <SettingRow
          label="Hold to dictate"
          description="Keep the shortcut held while speaking; release to finish."
        >
          <Switch
            checked={settings.dictationHoldEnabled}
            label="Hold to dictate"
            onChange={(value) => void save({ dictationHoldEnabled: value })}
          />
        </SettingRow>
        <SettingRow
          label="Hold threshold"
          description="How long (ms) a press must last to count as a hold instead of a tap."
        >
          <NumberPreference
            label="Hold threshold"
            min={50}
            max={5000}
            value={settings.dictationHoldThresholdMs}
            onChange={(value) => save({ dictationHoldThresholdMs: value })}
          />
        </SettingRow>
        <SettingRow label="Microphone">
          {microphones.length === 0 ? (
            <span {...stylex.props(styles.empty)}>
              No microphones found. Check the system sound settings.
            </span>
          ) : (
            <select
              aria-label="Microphone"
              onChange={(event) => void save({ microphoneId: event.target.value || null })}
              value={
                microphones.some((device) => device.id === (settings.microphoneId ?? "")) ||
                settings.microphoneId === null
                  ? (settings.microphoneId ?? "")
                  : ""
              }
              {...stylex.props(styles.field)}
            >
              <option value="">System Default</option>
              {microphones
                .filter((device) => device.id !== "default")
                .map((device) => (
                  <option key={device.id} value={device.id}>
                    {device.name}
                  </option>
                ))}
            </select>
          )}
        </SettingRow>
      </SettingsGroup>
      <SettingsGroup>
        <SettingRow
          label="Improve dictated text with AI"
          description="Cleans punctuation and obvious filler words without changing your meaning."
        >
          <Switch
            checked={settings.improveDictationWithAi}
            label="Improve dictated text with AI"
            onChange={(value) => void save({ improveDictationWithAi: value })}
          />
        </SettingRow>
        {settings.improveDictationWithAi ? (
          <SettingRow
            label="Cleanup model"
            description="Which AI model tidies the transcript. Defaults to your Writing Tools models, in order."
          >
            <DictationCleanupModel save={save} settings={settings} />
          </SettingRow>
        ) : null}
        <VocabularySummaryRow settings={settings} onVocabulary={onVocabulary} />
        <SettingRow label="Sound feedback" description="Play restrained start and finish sounds.">
          <Switch
            checked={settings.soundFeedback}
            label="Sound feedback"
            onChange={(value) => void save({ soundFeedback: value })}
          />
        </SettingRow>
      </SettingsGroup>
    </SettingsContent>
  );
}

function languageName(language: SpeechLanguage): string {
  if (language.downloadable && !language.installed) return `${language.name} — download required`;
  return language.name;
}

function dictationLanguageDescription(
  engine: AppSettings["speechEngine"],
  platform: AppContext["platform"],
): string {
  if (engine === "local") {
    return "On-device models detect the spoken language automatically; set this only to force one.";
  }
  if (engine === "voz") {
    return "Voz checks the spoken language on-device before transcription. Automatic needs a reliable match; otherwise Kivo asks you to choose another engine.";
  }
  if (platform === "windows") {
    return "Automatic uses the system speech language. Only installed desktop speech languages can start dictation — install one in Windows Settings → Time & language → Speech.";
  }
  return "Automatic follows the current input language when supported.";
}

function NumberPreference({
  label,
  value,
  min,
  max,
  onChange,
}: {
  readonly label: string;
  readonly value: number;
  readonly min: number;
  readonly max: number;
  readonly onChange: (value: number) => Promise<void>;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      aria-label={label}
      max={max}
      min={min}
      onBlur={(event) => {
        const next = event.currentTarget.valueAsNumber;
        setDraft(null);
        if (Number.isFinite(next) && next !== value)
          void onChange(Math.min(max, Math.max(min, next)));
      }}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Escape") event.currentTarget.value = String(value);
        if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
      }}
      type="number"
      value={draft ?? value}
      {...stylex.props(styles.field, styles.numberField)}
    />
  );
}
