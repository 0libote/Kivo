import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";
import { type AppContext, type AppSettings } from "../../types";
import { DictationVocabulary } from "./DictationVocabulary";
import { type SaveSettings, SettingRow, SettingsContent, SettingsGroup } from "./settings-layout";
import { styles } from "./settings-styles";
import { normalizeVocabularySetting } from "./vocabulary";
import { WritingPresetList } from "./WritingPresetEditor";
export function WritingSection({
  context,
  settings,
  save,
  onVocabulary,
}: {
  readonly context: AppContext;
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly onVocabulary: () => void;
}) {
  return (
    <SettingsContent
      title="Writing Tools"
      subtitle="Choose the actions shown when you work with selected text."
    >
      <SettingsGroup>
        <SettingRow label="Shortcut">
          <ShortcutRecorder
            label="Writing Tools shortcut"
            onChange={(writingShortcut) => save({ writingShortcut })}
            platform={context.platform}
            value={settings.writingShortcut}
          />
        </SettingRow>
        <VocabularySummaryRow settings={settings} onVocabulary={onVocabulary} />
      </SettingsGroup>
      <SettingsGroup header="Presets">
        <WritingPresetList save={save} settings={settings} />
      </SettingsGroup>
    </SettingsContent>
  );
}

export function VocabularySection({
  settings,
  save,
  setNotice,
}: {
  readonly settings: AppSettings;
  readonly save: SaveSettings;
  readonly setNotice: (value: string | null) => void;
}) {
  return (
    <SettingsContent
      title="Custom words"
      subtitle="Teach Kivo your names, acronyms, and terms once — dictation prefers your exact spelling and AI cleanup plus Writing Tools leave them alone."
    >
      <SettingsGroup>
        <SettingRow
          label="Words and meanings"
          description="Add what each word means so the AI uses it correctly, not just spells it right."
          stacked
        >
          <DictationVocabulary onNotice={setNotice} save={save} settings={settings} />
        </SettingRow>
      </SettingsGroup>
      <p {...stylex.props(styles.note)}>
        Custom words shape dictated text on every transcription engine and are protected from AI
        cleanup and Writing Tools. They never leave this computer except inside the AI requests you
        already send to your chosen provider.
      </p>
    </SettingsContent>
  );
}

/** Compact cross-link shown where custom words apply (Dictation, Writing Tools). */
export function VocabularySummaryRow({
  settings,
  onVocabulary,
}: {
  readonly settings: AppSettings;
  readonly onVocabulary: () => void;
}) {
  const count = normalizeVocabularySetting(settings.dictationVocabulary).length;
  const unit = count === 1 ? "word" : "words";
  const description =
    count === 0
      ? "No words taught yet. Names and terms you add are preferred by dictation and protected from AI cleanup."
      : `${count} ${unit} taught — preferred by dictation and protected from AI cleanup.`;
  return (
    <SettingRow label="Custom words" description={description}>
      <Button label="Manage words" onClick={onVocabulary} size="sm" variant="secondary" />
    </SettingRow>
  );
}
