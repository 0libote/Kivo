use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::ai::WritingAction;

pub const SETTINGS_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub schema_version: u32,
    pub general: GeneralSettings,
    pub dictation: DictationSettings,
    pub writing_tools: WritingToolsSettings,
    pub ai: AiSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            general: GeneralSettings::default(),
            dictation: DictationSettings::default(),
            writing_tools: WritingToolsSettings::default(),
            ai: AiSettings::default(),
        }
    }
}

impl AppSettings {
    pub fn validate_and_normalize(mut self) -> Result<Self, SettingsError> {
        if self.schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedVersion(self.schema_version));
        }
        let old_schema_version = self.schema_version;
        if old_schema_version < 2 {
            self.migrate_v2_defaults();
        }
        self.schema_version = SETTINGS_SCHEMA_VERSION;
        self.general.validate()?;
        self.dictation.migrate_foreign_default();
        self.dictation.normalize();
        self.dictation.validate()?;
        self.writing_tools.normalize();
        self.ai.normalize();
        self.ai.validate()?;
        // Per-preset model priorities live on the same provider as Writing
        // Tools. A provider switch can leave stored ids unusable, so normalize
        // them against the active provider (an emptied list follows the global
        // queue again).
        let provider = self.ai.provider;
        for preset in &mut self.writing_tools.presets {
            if !preset.models.is_empty() {
                preset.models = crate::ai::normalize_model_list(provider, &preset.models);
            }
        }
        // The cleanup override is a model on the same provider as Writing
        // Tools. A provider switch can leave a stored id unusable, so drop it
        // and fall back to the writing queue instead of failing silently on
        // every dictation.
        self.dictation.cleanup_model = self
            .dictation
            .cleanup_model
            .take()
            .filter(|id| crate::ai::is_usable_model_for(self.ai.provider, id));
        Ok(self)
    }

    fn migrate_v2_defaults(&mut self) {
        // Kivo previously put a coding model first for Go. Move only that
        // exact old default; an explicit model choice saved under v2 stays
        // untouched.
        if self.ai.provider == crate::ai::AiProvider::Go
            && self.ai.models.len() == 1
            && self.ai.models[0].trim() == "kimi-k2.7-code"
        {
            self.ai.models = vec![self.ai.provider.default_model().to_owned()];
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub theme: ThemePreference,
    pub show_flow_bar_while_idle: bool,
    pub start_in_background: bool,
    pub onboarding_complete: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            theme: ThemePreference::System,
            show_flow_bar_while_idle: false,
            start_in_background: true,
            onboarding_complete: false,
        }
    }
}

impl GeneralSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// Variants for the *other* desktop are only constructed by unit tests (each
// CI host executes both directions); without this, `clippy -D warnings`
// would flag them as never constructed on single-host builds.
#[allow(dead_code)]
pub(crate) enum HostPlatform {
    Windows,
    Linux,
    Other,
}

impl HostPlatform {
    pub(crate) fn current() -> Self {
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "linux")]
        return Self::Linux;
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        return Self::Other;
    }
}

/// Native hold shortcut per host, parameterized so every CI platform can
/// assert every other platform's default (a `#[cfg]`-gated test only ever
/// exercises its own host and lets the other default drift silently).
///
/// Linux is the dev/test bench: it uses a portable global-shortcut
/// accelerator (`Control+Alt+Space`) that differs from the Windows Ctrl+Win hold, so the same AppCore path is exercised
/// on both hosts.
pub(crate) fn dictation_default_for(host: HostPlatform) -> ShortcutBinding {
    match host {
        HostPlatform::Windows => ShortcutBinding::new("Ctrl+Meta"),
        HostPlatform::Linux | HostPlatform::Other => ShortcutBinding::new("Control+Alt+Space"),
    }
}

/// Portable Writing Tools shortcut per host. Same cross-host testability
/// rationale as [`dictation_default_for`]. Linux reuses the Windows
/// accelerator so writing-tools behavior matches between the test bench and
/// the Windows target.
pub(crate) fn writing_tools_default_for(host: HostPlatform) -> ShortcutBinding {
    match host {
        HostPlatform::Windows | HostPlatform::Linux | HostPlatform::Other => {
            ShortcutBinding::new("Ctrl+Space")
        }
    }
}

/// Replacement for a foreign native dictation default carried over in a
/// settings file, or `None` when the accelerator is valid on `host`.
/// Pure over `host` so one test run covers legacy-settings migration.
fn foreign_default_replacement(accelerator: &str, host: HostPlatform) -> Option<ShortcutBinding> {
    match host {
        HostPlatform::Windows if accelerator == "Fn" => Some(dictation_default_for(host)),
        // The Linux test bench has no native hold monitor: a carried-over
        // Fn / Ctrl+Win default would fail global-shortcut registration, so
        // migrate it to the portable Linux default instead of leaving
        // dictation silently dead.
        HostPlatform::Linux | HostPlatform::Other
            if accelerator == "Fn"
                || accelerator == "Ctrl+Meta"
                || accelerator == "Control+Super" =>
        {
            Some(dictation_default_for(host))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictationSettings {
    pub shortcut: ShortcutBinding,
    pub microphone_id: Option<String>,
    pub improve_with_ai: bool,
    pub language: LanguagePreference,
    pub sound_feedback: bool,
    pub tap_enabled: bool,
    pub hold_enabled: bool,
    pub hold_threshold_ms: u64,
    /// Which recognizer to use. `System` keeps the OS engine (the default so
    /// existing installs are unchanged); `Local` runs a downloaded model.
    pub speech_engine: SpeechEnginePreference,
    /// Selected on-device model id. `None` resolves to the recommended model.
    pub local_speech_model: Option<String>,
    /// Optional model id for the AI dictation cleanup. `None` uses the Writing
    /// Tools ordered queue; a value pins cleanup to one model (typically a
    /// smaller/cheaper one) on the same AI provider.
    pub cleanup_model: Option<String>,
    /// Custom vocabulary: exact spellings the transcriber should prefer and
    /// AI cleanup / Writing Tools must not "correct", each with optional
    /// context for the AI. Empty by default.
    #[serde(default, deserialize_with = "deserialize_vocabulary")]
    pub vocabulary: Vec<VocabularyEntry>,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            shortcut: ShortcutBinding::dictation_default(),
            microphone_id: None,
            improve_with_ai: true,
            language: LanguagePreference::Auto,
            sound_feedback: true,
            tap_enabled: true,
            hold_enabled: true,
            hold_threshold_ms: 350,
            speech_engine: SpeechEnginePreference::System,
            local_speech_model: None,
            cleanup_model: None,
            vocabulary: Vec::new(),
        }
    }
}

impl DictationSettings {
    /// A settings file carried over from the other desktop OS keeps its
    /// dictation shortcut, but the native hold shortcuts are OS-exclusive:
    /// "Fn" only exists on macOS and "Ctrl+Meta" only on Windows. A foreign
    /// native default would fail registration and leave dictation silently
    /// dead, so migrate exactly those while custom shortcuts (handled by the
    /// portable global-shortcut plugin on both platforms) pass through.
    fn migrate_foreign_default(&mut self) {
        let host = HostPlatform::current();
        if let Some(replacement) = foreign_default_replacement(&self.shortcut.accelerator, host) {
            self.shortcut = replacement;
        }
    }

    fn normalize(&mut self) {
        // A zero threshold would classify every press as a hold and break
        // tap-to-dictate; 50 ms is the smallest distinguishable hold.
        self.hold_threshold_ms = self.hold_threshold_ms.clamp(50, 5000);
        if let Some(id) = self.local_speech_model.as_deref()
            && id.trim().is_empty()
        {
            self.local_speech_model = None;
        }
        if let Some(id) = self.cleanup_model.as_deref() {
            let trimmed = id.trim();
            self.cleanup_model = (!trimmed.is_empty()).then(|| trimmed.to_owned());
        }
        // Custom vocabulary: trim, drop empties, fold duplicates
        // case-insensitively (first spelling wins; a later duplicate still
        // fills in a missing meaning), and cap the count so an imported list
        // can neither bloat the settings file nor dilute the model hint
        // into noise.
        let mut index_by_word: HashMap<String, usize> = HashMap::new();
        let mut normalized: Vec<VocabularyEntry> = Vec::new();
        for entry in self.vocabulary.drain(..) {
            let word = truncate(entry.word.trim(), MAX_VOCABULARY_WORD_CHARS);
            if word.is_empty() {
                continue;
            }
            let meaning = truncate(entry.meaning.trim(), MAX_VOCABULARY_MEANING_CHARS);
            let key = word.to_lowercase();
            if let Some(&index) = index_by_word.get(&key) {
                if normalized[index].meaning.is_empty() && !meaning.is_empty() {
                    normalized[index].meaning = meaning;
                }
                continue;
            }
            if normalized.len() >= MAX_VOCABULARY_WORDS {
                continue;
            }
            index_by_word.insert(key, normalized.len());
            normalized.push(VocabularyEntry { word, meaning });
        }
        self.vocabulary = normalized;
    }

    fn validate(&self) -> Result<(), SettingsError> {
        self.shortcut.validate()?;
        if let Some(id) = &self.microphone_id
            && (id.trim().is_empty() || id.len() > 512)
        {
            return Err(SettingsError::InvalidMicrophone);
        }
        if let LanguagePreference::Locale { tag } = &self.language
            && (tag.trim().is_empty() || tag.len() > 64)
        {
            return Err(SettingsError::InvalidLanguage);
        }
        if let Some(id) = &self.local_speech_model
            && id.len() > 128
        {
            return Err(SettingsError::InvalidLocalModel);
        }
        if let Some(id) = &self.cleanup_model
            && id.len() > 128
        {
            return Err(SettingsError::InvalidAiModel);
        }
        if self.vocabulary.len() > MAX_VOCABULARY_WORDS
            || self.vocabulary.iter().any(|entry| {
                entry.word.chars().count() > MAX_VOCABULARY_WORD_CHARS
                    || entry.meaning.chars().count() > MAX_VOCABULARY_MEANING_CHARS
            })
        {
            return Err(SettingsError::InvalidVocabulary);
        }
        Ok(())
    }
}

/// Which recognizer powers dictation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeechEnginePreference {
    #[default]
    System,
    Local,
    Voz,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum LanguagePreference {
    #[default]
    Auto,
    Locale {
        tag: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
    pub accelerator: String,
}

impl ShortcutBinding {
    pub fn new(accelerator: impl Into<String>) -> Self {
        Self {
            accelerator: accelerator.into(),
        }
    }

    pub fn dictation_default() -> Self {
        dictation_default_for(HostPlatform::current())
    }

    pub fn writing_tools_default() -> Self {
        writing_tools_default_for(HostPlatform::current())
    }

    fn validate(&self) -> Result<(), SettingsError> {
        let value = self.accelerator.trim();
        if value.is_empty() || value.len() > 96 || value.chars().any(char::is_control) {
            return Err(SettingsError::InvalidShortcut);
        }
        Ok(())
    }
}

impl Default for ShortcutBinding {
    fn default() -> Self {
        Self::writing_tools_default()
    }
}

/// One user-editable Writing Tools preset. Built-in presets carry a stable id
/// (`proofread`, …); custom ones use a generated id. Only overridden built-ins
/// and custom presets are stored — untouched built-ins fall back to the
/// defaults mirrored in `src/features/writing-tools/presets.ts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WritingPresetSettings {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    /// The task instruction ("what it should do").
    #[serde(default)]
    pub instruction: String,
    /// Advanced override: the full system-prompt template. `None` uses the
    /// default template with `{{instruction}}` / `{{outputRule}}` placeholders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default)]
    pub replaces_selection: bool,
    /// Per-preset model priority. Empty follows the global AI model queue.
    #[serde(default)]
    pub models: Vec<String>,
}

/// Custom dictation vocabulary ("Custom words" in Settings → Dictation).
/// Names, acronyms, and terms the transcriber should prefer with the user's
/// exact spelling, each with optional context ("what it means") for the AI.
/// Stored as entries (no wrong→right correction rules): engines receive the
/// words as hints and the AI prompts carry words plus meanings as protected
/// terms.
pub const MAX_VOCABULARY_WORDS: usize = 200;
pub const MAX_VOCABULARY_WORD_CHARS: usize = 60;
pub const MAX_VOCABULARY_MEANING_CHARS: usize = 200;

/// One custom word plus optional context for the AI. Serialized as an object;
/// a bare string still deserializes (early settings files stored words only).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VocabularyEntry {
    pub word: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub meaning: String,
}

fn deserialize_vocabulary<'de, D>(deserializer: D) -> Result<Vec<VocabularyEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WordOrEntry {
        Word(String),
        Entry(VocabularyEntry),
    }
    let items = Vec::<WordOrEntry>::deserialize(deserializer)?;
    Ok(items
        .into_iter()
        .map(|item| match item {
            WordOrEntry::Word(word) => VocabularyEntry {
                word,
                meaning: String::new(),
            },
            WordOrEntry::Entry(entry) => entry,
        })
        .collect())
}

/// Caps mirrored by `src/features/writing-tools/presets.ts`.
pub const MAX_WRITING_PRESETS: usize = 24;
const MAX_PRESET_LABEL: usize = 60;
const MAX_PRESET_TEXT: usize = 8_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WritingToolsSettings {
    pub shortcut: ShortcutBinding,
    /// Ordered ids of the presets shown in the popup. Built-in ids are shared
    /// with the frontend; custom ids index [`Self::presets`].
    pub enabled_actions: Vec<String>,
    /// Overridden built-ins and custom presets. Definitions are inert to the
    /// backend prompt builder (the frontend resolves and sends the system
    /// instruction), but they persist here so both windows agree.
    #[serde(default)]
    pub presets: Vec<WritingPresetSettings>,
}

impl Default for WritingToolsSettings {
    fn default() -> Self {
        Self {
            shortcut: ShortcutBinding::writing_tools_default(),
            enabled_actions: default_writing_action_ids(),
            presets: Vec::new(),
        }
    }
}

fn default_writing_action_ids() -> Vec<String> {
    WritingAction::all()
        .iter()
        .map(|action| action.as_str().to_owned())
        .collect()
}

impl WritingToolsSettings {
    fn normalize(&mut self) {
        let mut seen = HashSet::new();
        self.enabled_actions = self
            .enabled_actions
            .drain(..)
            .map(|id| WritingAction::canonical_id(&id))
            .filter(|id| !id.is_empty() && seen.insert(id.clone()))
            .take(MAX_WRITING_PRESETS)
            .collect();

        let mut seen_presets = HashSet::new();
        self.presets = self
            .presets
            .drain(..)
            .filter_map(|mut preset| {
                preset.id = preset.id.trim().to_owned();
                preset.label = truncate(preset.label.trim(), MAX_PRESET_LABEL);
                preset.description = truncate(preset.description.trim(), MAX_PRESET_LABEL);
                preset.icon = truncate(preset.icon.trim(), MAX_PRESET_LABEL);
                preset.instruction = truncate(preset.instruction.trim(), MAX_PRESET_TEXT);
                preset.template = preset
                    .template
                    .take()
                    .map(|value| truncate(value.trim(), MAX_PRESET_TEXT))
                    .filter(|value| !value.is_empty());
                let mut seen_models = HashSet::new();
                preset.models = preset
                    .models
                    .drain(..)
                    .map(|model| model.trim().to_owned())
                    .filter(|model| !model.is_empty() && seen_models.insert(model.clone()))
                    .take(crate::ai::MAX_AI_MODELS)
                    .collect();
                if preset.id.is_empty() || !seen_presets.insert(preset.id.clone()) {
                    return None;
                }
                Some(preset)
            })
            .take(MAX_WRITING_PRESETS)
            .collect();
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    value.chars().take(max_chars).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    #[serde(default)]
    pub provider: crate::ai::AiProvider,
    /// Ordered failover queue: tried top to bottom until one succeeds.
    #[serde(default)]
    pub models: Vec<String>,
    /// Fast/balanced/deep reasoning preset. Providers ignore it when the
    /// selected model does not expose a compatible control.
    #[serde(default)]
    pub reasoning_mode: crate::ai::AiReasoningMode,
    /// Custom provider only: OpenAI-compatible base URL
    /// (e.g. Ollama `http://localhost:11434/v1`). `None` means the Ollama
    /// default. Ignored by the other providers.
    #[serde(default)]
    pub custom_base_url: Option<String>,
    /// Pre-queue settings files stored a single `model` plus an optional
    /// `backupModel`. Captured here so the first load after upgrading
    /// migrates them into `models` instead of dropping the user's choice;
    /// never serialized back out.
    #[serde(default, rename = "model", skip_serializing)]
    legacy_model: Option<String>,
    #[serde(default, rename = "backupModel", skip_serializing)]
    legacy_backup_model: Option<String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: crate::ai::AiProvider::default(),
            models: vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()],
            reasoning_mode: crate::ai::AiReasoningMode::default(),
            custom_base_url: None,
            legacy_model: None,
            legacy_backup_model: None,
        }
    }
}

impl AiSettings {
    pub fn new(
        provider: crate::ai::AiProvider,
        models: Vec<String>,
        reasoning_mode: crate::ai::AiReasoningMode,
        custom_base_url: Option<String>,
    ) -> Self {
        Self {
            provider,
            models,
            reasoning_mode,
            custom_base_url,
            legacy_model: None,
            legacy_backup_model: None,
        }
    }

    fn normalize(&mut self) {
        // One-time upgrade: fold the legacy primary/backup pair into the
        // queue. An explicit (possibly empty) `models` array always wins.
        if self.models.is_empty() {
            let mut migrated = Vec::new();
            for candidate in [self.legacy_model.take(), self.legacy_backup_model.take()] {
                if let Some(candidate) = candidate
                    && !candidate.trim().is_empty()
                {
                    migrated.push(candidate);
                }
            }
            self.models = migrated;
        } else {
            self.legacy_model = None;
            self.legacy_backup_model = None;
        }
        self.models = crate::ai::normalize_model_list(self.provider, &self.models);
        // The custom endpoint is preserved across provider switches (other
        // providers ignore it) so switching back doesn't lose the URL.
        self.custom_base_url = crate::ai::normalize_base_url(self.custom_base_url.as_deref());
    }

    fn validate(&self) -> Result<(), SettingsError> {
        if self.models.is_empty()
            || self.models.len() > crate::ai::MAX_AI_MODELS
            || self
                .models
                .iter()
                .any(|model| !crate::ai::is_usable_model_for(self.provider, model))
        {
            return Err(SettingsError::InvalidAiModel);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    InvalidJson(serde_json::Error),
    UnsupportedVersion(u32),
    InvalidShortcut,
    InvalidMicrophone,
    InvalidLanguage,
    InvalidLocalModel,
    InvalidAiModel,
    InvalidVocabulary,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Io(_) => "Settings could not be saved or loaded.",
            Self::InvalidJson(_) => "The settings file is invalid.",
            Self::UnsupportedVersion(version) => {
                return write!(
                    formatter,
                    "The settings file uses unsupported schema version {version}."
                );
            }
            Self::InvalidShortcut => "The configured shortcut is invalid.",
            Self::InvalidMicrophone => "The configured microphone is invalid.",
            Self::InvalidLanguage => "The configured language is invalid.",
            Self::InvalidLocalModel => "The configured speech model is invalid.",
            Self::InvalidAiModel => "The configured AI model is not supported.",
            Self::InvalidVocabulary => "The custom vocabulary list is invalid.",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SettingsError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidJson(value)
    }
}

pub struct SettingsRepository {
    path: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SettingsRuntimeError {
    ShortcutUnavailable,
    AutostartUnavailable,
}

impl fmt::Display for SettingsRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ShortcutUnavailable => "That shortcut is already in use.",
            Self::AutostartUnavailable => "Launch at login could not be updated.",
        })
    }
}

impl std::error::Error for SettingsRuntimeError {}

/// Applies preferences that have an operating-system side effect.
pub trait SettingsRuntime: Send + Sync {
    fn apply(
        &self,
        previous: &AppSettings,
        updated: &AppSettings,
    ) -> Result<(), SettingsRuntimeError>;
}

impl SettingsRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<AppSettings, SettingsError> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<AppSettings>(&bytes)?.validate_and_normalize(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppSettings::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let settings = settings.clone().validate_and_normalize()?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let bytes = serde_json::to_vec_pretty(&settings)?;
        let temporary_path = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&temporary_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;

        replace_file(&temporary_path, &self.path)?;
        Ok(())
    }
}

fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    // std::fs::rename replaces existing files on both platforms. Removing
    // the destination first loses the user's settings if the rename fails.
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests;
