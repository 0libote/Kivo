use std::{
    collections::HashSet,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::ai::WritingAction;

pub const SETTINGS_SCHEMA_VERSION: u32 = 2;

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
    Macos,
    Windows,
    Linux,
    Other,
}

impl HostPlatform {
    pub(crate) fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Self::Macos;
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(target_os = "linux")]
        return Self::Linux;
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        return Self::Other;
    }
}

/// Native hold shortcut per host, parameterized so every CI platform can
/// assert every other platform's default (a `#[cfg]`-gated test only ever
/// exercises its own host and lets the other default drift silently).
///
/// Linux is the dev/test bench: it uses a portable global-shortcut
/// accelerator (`Control+Alt+Space`) that never collides with the macOS Fn
/// hold or the Windows Ctrl+Win hold, so the same AppCore path is exercised
/// on all three hosts.
pub(crate) fn dictation_default_for(host: HostPlatform) -> ShortcutBinding {
    match host {
        HostPlatform::Macos => ShortcutBinding::new("Fn"),
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
        HostPlatform::Macos => ShortcutBinding::new("Ctrl+Shift+Space"),
        HostPlatform::Windows | HostPlatform::Linux | HostPlatform::Other => {
            ShortcutBinding::new("Ctrl+Space")
        }
    }
}

/// Replacement for a foreign native dictation default carried over in a
/// settings file, or `None` when the accelerator is valid on `host`.
/// Pure over `host` so one test run covers both migration directions.
fn foreign_default_replacement(accelerator: &str, host: HostPlatform) -> Option<ShortcutBinding> {
    match host {
        HostPlatform::Windows if accelerator == "Fn" => Some(dictation_default_for(host)),
        HostPlatform::Macos if accelerator == "Ctrl+Meta" || accelerator == "Control+Super" => {
            Some(dictation_default_for(host))
        }
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
        Ok(())
    }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WritingToolsSettings {
    pub shortcut: ShortcutBinding,
    pub enabled_actions: Vec<WritingAction>,
    pub popup_anchor: PopupAnchor,
    pub popup_fixed_x: f64,
    pub popup_fixed_y: f64,
    pub popup_width: f64,
    pub popup_height: f64,
    pub allow_manual_text: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PopupAnchor {
    #[default]
    Cursor,
    Selection,
    Fixed,
}

impl Default for WritingToolsSettings {
    fn default() -> Self {
        Self {
            shortcut: ShortcutBinding::writing_tools_default(),
            enabled_actions: WritingAction::all().to_vec(),
            popup_anchor: PopupAnchor::default(),
            popup_fixed_x: 480.0,
            popup_fixed_y: 320.0,
            popup_width: 380.0,
            popup_height: 460.0,
            allow_manual_text: true,
        }
    }
}

impl WritingToolsSettings {
    fn normalize(&mut self) {
        let mut seen = HashSet::new();
        self.enabled_actions.retain(|action| seen.insert(*action));
        // A single fixed size for every popup mode keeps placement stable;
        // per-mode resizing is what made the old popup jump around.
        self.popup_width = self.popup_width.clamp(280.0, 800.0);
        self.popup_height = self.popup_height.clamp(200.0, 800.0);
        if !self.popup_fixed_x.is_finite() {
            self.popup_fixed_x = 480.0;
        }
        if !self.popup_fixed_y.is_finite() {
            self.popup_fixed_y = 320.0;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    #[serde(default)]
    pub provider: crate::ai::AiProvider,
    /// Ordered failover queue: tried top to bottom until one succeeds.
    #[serde(default)]
    pub models: Vec<String>,
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
        custom_base_url: Option<String>,
    ) -> Self {
        Self {
            provider,
            models,
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
    InvalidAiModel,
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
            Self::InvalidAiModel => "The configured AI model is not supported.",
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
mod tests {
    use std::{env, fs, time::SystemTime};

    use super::{
        AppSettings, HostPlatform, SettingsRepository, ShortcutBinding, ThemePreference,
        dictation_default_for, foreign_default_replacement, writing_tools_default_for,
    };
    use crate::ai::WritingAction;

    fn temporary_settings_path() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("kivo-settings-{unique}.json"))
    }

    #[test]
    fn missing_settings_use_defaults() {
        let path = temporary_settings_path();
        let repository = SettingsRepository::new(&path);
        assert_eq!(repository.load().unwrap(), AppSettings::default());
    }

    #[test]
    fn settings_round_trip_without_a_secret_field() {
        let path = temporary_settings_path();
        let repository = SettingsRepository::new(&path);
        let mut settings = AppSettings::default();
        settings.general.theme = ThemePreference::Dark;
        settings.writing_tools.enabled_actions = vec![WritingAction::Proofread];

        repository.save(&settings).unwrap();
        assert_eq!(repository.load().unwrap(), settings);

        settings.general.theme = ThemePreference::Light;
        repository.save(&settings).unwrap();
        assert_eq!(repository.load().unwrap(), settings);
        let persisted = fs::read_to_string(&path).unwrap();
        assert!(!persisted.to_ascii_lowercase().contains("api_key"));
        assert!(!persisted.to_ascii_lowercase().contains("apikey"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn foreign_native_dictation_shortcut_migrates_to_the_host_default() {
        // Host-parameterized: every CI platform executes both migration
        // directions, so a default changed on one OS without its counterpart
        // fails fast instead of surfacing weeks later on the other OS.
        // Windows host: macOS "Fn" migrates; everything else passes through.
        assert_eq!(
            foreign_default_replacement("Fn", HostPlatform::Windows),
            Some(ShortcutBinding::new("Ctrl+Meta"))
        );
        assert_eq!(
            foreign_default_replacement("Ctrl+Alt+D", HostPlatform::Windows),
            None
        );
        assert_eq!(
            foreign_default_replacement("Ctrl+Meta", HostPlatform::Windows),
            None
        );
        // macOS host: both Windows spellings ("Ctrl+Meta" as stored,
        // "Control+Super" as normalized in shell.rs) migrate.
        assert_eq!(
            foreign_default_replacement("Ctrl+Meta", HostPlatform::Macos),
            Some(ShortcutBinding::new("Fn"))
        );
        assert_eq!(
            foreign_default_replacement("Control+Super", HostPlatform::Macos),
            Some(ShortcutBinding::new("Fn"))
        );
        assert_eq!(
            foreign_default_replacement("Ctrl+Alt+D", HostPlatform::Macos),
            None
        );
        assert_eq!(foreign_default_replacement("Fn", HostPlatform::Macos), None);

        // Linux host: both native defaults migrate to the portable default.
        assert_eq!(
            foreign_default_replacement("Fn", HostPlatform::Linux),
            Some(ShortcutBinding::new("Control+Alt+Space"))
        );
        assert_eq!(
            foreign_default_replacement("Ctrl+Meta", HostPlatform::Linux),
            Some(ShortcutBinding::new("Control+Alt+Space"))
        );
        assert_eq!(
            foreign_default_replacement("Control+Super", HostPlatform::Linux),
            Some(ShortcutBinding::new("Control+Alt+Space"))
        );
        assert_eq!(
            foreign_default_replacement("Ctrl+Alt+D", HostPlatform::Linux),
            None
        );

        // End-to-end through normalization on the current host: the host's
        // own default survives while custom shortcuts pass through untouched.
        let mut settings = AppSettings::default();
        settings.dictation.shortcut = ShortcutBinding::new("Ctrl+Alt+D");
        let migrated = settings.validate_and_normalize().unwrap();
        assert_eq!(
            migrated.dictation.shortcut,
            ShortcutBinding::new("Ctrl+Alt+D")
        );
    }

    #[test]
    fn platform_defaults_match_the_frontend_contract() {
        // Mirror of src/types.ts defaultSettings() and the frontend
        // platform-defaults test. If either side changes a default, update
        // both together (and scripts/check-platform-parity.ts enforces it in
        // CI without compiling).
        assert_eq!(
            dictation_default_for(HostPlatform::Macos),
            ShortcutBinding::new("Fn")
        );
        assert_eq!(
            dictation_default_for(HostPlatform::Windows),
            ShortcutBinding::new("Ctrl+Meta")
        );
        assert_eq!(
            dictation_default_for(HostPlatform::Linux),
            ShortcutBinding::new("Control+Alt+Space")
        );
        assert_eq!(
            writing_tools_default_for(HostPlatform::Macos),
            ShortcutBinding::new("Ctrl+Shift+Space")
        );
        assert_eq!(
            writing_tools_default_for(HostPlatform::Windows),
            ShortcutBinding::new("Ctrl+Space")
        );
        assert_eq!(
            writing_tools_default_for(HostPlatform::Linux),
            ShortcutBinding::new("Ctrl+Space")
        );
        // Linux dictation and writing defaults must differ: sharing one
        // accelerator would register the same global shortcut twice.
        assert_ne!(
            dictation_default_for(HostPlatform::Linux),
            writing_tools_default_for(HostPlatform::Linux)
        );
    }

    #[test]
    fn duplicate_actions_are_normalized() {
        let mut settings = AppSettings::default();
        settings.writing_tools.enabled_actions = vec![
            WritingAction::Proofread,
            WritingAction::Proofread,
            WritingAction::Concise,
        ];

        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(
            settings.writing_tools.enabled_actions,
            vec![WritingAction::Proofread, WritingAction::Concise]
        );
    }

    #[test]
    fn popup_geometry_is_clamped_and_old_files_still_load() {
        let mut settings = AppSettings::default();
        settings.writing_tools.popup_width = 5000.0;
        settings.writing_tools.popup_height = 10.0;
        settings.writing_tools.popup_fixed_x = f64::NAN;

        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.writing_tools.popup_width, 800.0);
        assert_eq!(settings.writing_tools.popup_height, 200.0);
        assert_eq!(settings.writing_tools.popup_fixed_x, 480.0);

        // Files written before the popup fields existed deserialize via
        // serde defaults and keep the cursor-anchored default.
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "general": {},
            "dictation": { "shortcut": { "accelerator": "Fn" } },
            "writingTools": {
                "shortcut": { "accelerator": "Ctrl+Shift+Space" },
                "enabledActions": ["proofread"],
            },
        });
        let settings: AppSettings = serde_json::from_value(legacy).unwrap();
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(
            settings.writing_tools.popup_anchor,
            super::PopupAnchor::Cursor
        );
        assert!(settings.writing_tools.allow_manual_text);
    }

    #[test]
    fn ai_model_defaults_and_legacy_files() {
        // Default matches the frontend contract (src/types.ts DEFAULT_AI_MODEL).
        assert_eq!(
            AppSettings::default().ai.models,
            vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
        );

        // Supported ids survive normalization (trimmed), including new ids
        // that were never in the curated suggestion list.
        let mut settings = AppSettings::default();
        settings.ai.models = vec![
            "  gemini-2.5-flash  ".into(),
            "models/gemini-4.0-flash".into(),
        ];
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.models,
            vec!["gemini-2.5-flash", "gemini-4.0-flash"]
        );

        // Duplicates collapse (first wins) and unknown / TTS / image ids are
        // dropped; an emptied queue falls back to the default instead of
        // bricking AI requests.
        let mut settings = AppSettings::default();
        settings.ai.models = vec![
            "gemini-2.5-flash".into(),
            "gemini-2.5-flash".into(),
            "has spaces!".into(),
            "gemini-2.5-flash-preview-tts".into(),
        ];
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.models,
            vec!["gemini-2.5-flash".to_owned()]
        );
        let mut settings = AppSettings::default();
        settings.ai.models = vec![];
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.models,
            vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
        );

        // Files written before the ai section existed deserialize via serde
        // defaults and keep working.
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "general": {},
            "dictation": { "shortcut": { "accelerator": "Fn" } },
            "writingTools": {
                "shortcut": { "accelerator": "Ctrl+Shift+Space" },
                "enabledActions": ["proofread"],
            },
        });
        let settings: AppSettings = serde_json::from_value(legacy).unwrap();
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(
            settings.ai.models,
            vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
        );
    }

    #[test]
    fn go_default_upgrade_prefers_the_fast_model_without_overwriting_new_choices() {
        let old = AppSettings {
            schema_version: 1,
            ai: super::AiSettings {
                provider: crate::ai::AiProvider::Go,
                models: vec!["kimi-k2.7-code".into()],
                ..super::AiSettings::default()
            },
            ..AppSettings::default()
        };
        let old = old.validate_and_normalize().unwrap();
        assert_eq!(old.schema_version, super::SETTINGS_SCHEMA_VERSION);
        assert_eq!(old.ai.models, vec!["glm-5.3-flash"]);

        let mut explicit = AppSettings::default();
        explicit.ai.provider = crate::ai::AiProvider::Go;
        explicit.ai.models = vec!["kimi-k2.7-code".into()];
        let explicit = explicit.validate_and_normalize().unwrap();
        assert_eq!(explicit.ai.models, vec!["kimi-k2.7-code"]);
    }

    #[test]
    fn ai_legacy_primary_and_backup_migrate_into_the_queue() {
        // Pre-queue files stored `model` + `backupModel`: both migrate in
        // order, and the legacy keys are never written back out.
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "general": {},
            "dictation": { "shortcut": { "accelerator": "Fn" } },
            "writingTools": {
                "shortcut": { "accelerator": "Ctrl+Shift+Space" },
                "enabledActions": ["proofread"],
            },
            "ai": {
                "provider": "zen",
                "model": "opencode/kimi-k2.7-code",
                "backupModel": "glm-5.3-flash",
            },
        });
        let settings: AppSettings = serde_json::from_value(legacy).unwrap();
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.models, vec!["kimi-k2.7-code", "glm-5.3-flash"]);
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved["ai"].get("models").is_some());
        assert!(saved["ai"].get("model").is_none());
        assert!(saved["ai"].get("backupModel").is_none());

        // An explicit queue always wins over stale legacy fields.
        let mixed = serde_json::json!({
            "schemaVersion": 1,
            "ai": {
                "models": ["glm-5.3-flash"],
                "model": "gemini-2.5-flash",
                "backupModel": "gemini-2.5-flash-lite",
            },
        });
        let settings: AppSettings = serde_json::from_value(mixed).unwrap();
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.models, vec!["glm-5.3-flash".to_owned()]);
    }

    #[test]
    fn ai_queues_truncate_to_the_cap() {
        let mut settings = AppSettings::default();
        settings.ai.models = (0..crate::ai::MAX_AI_MODELS + 3)
            .map(|n| format!("gemini-test-{n}-flash"))
            .collect();
        let models = settings.validate_and_normalize().unwrap().ai.models;
        assert_eq!(models.len(), crate::ai::MAX_AI_MODELS);
        assert_eq!(models[0], "gemini-test-0-flash");
    }

    #[test]
    fn ai_provider_defaults_to_gemini_and_legacy_files_keep_working() {
        use crate::ai::AiProvider;
        assert_eq!(AppSettings::default().ai.provider, AiProvider::Gemini);
        assert_eq!(AppSettings::default().ai.custom_base_url, None);
        // Files written before the provider field existed deserialize via
        // serde defaults to Gemini, preserving the stored Gemini key slot.
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "general": {},
            "dictation": { "shortcut": { "accelerator": "Fn" } },
            "writingTools": {
                "shortcut": { "accelerator": "Ctrl+Shift+Space" },
                "enabledActions": ["proofread"],
            },
            "ai": { "model": "gemini-2.5-flash" },
        });
        let settings: AppSettings = serde_json::from_value(legacy).unwrap();
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.provider, AiProvider::Gemini);
        assert_eq!(settings.ai.models, vec!["gemini-2.5-flash".to_owned()]);
    }

    #[test]
    fn ai_provider_switch_falls_back_to_usable_models() {
        use crate::ai::AiProvider;
        // Zen keeps Gemini text ids (usable there) but drops TTS families.
        let mut settings = AppSettings::default();
        settings.ai.provider = AiProvider::Zen;
        settings.ai.models = vec!["gemini-2.5-flash".into()];
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.models, vec!["gemini-2.5-flash".to_owned()]);
        let mut settings = AppSettings::default();
        settings.ai.provider = AiProvider::Zen;
        settings.ai.models = vec!["gemini-2.5-flash-preview-tts".into()];
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(
            settings.ai.models,
            vec![AiProvider::Zen.default_model().to_owned()]
        );
        // Custom accepts local ids and normalizes the base URL.
        let mut settings = AppSettings::default();
        settings.ai.provider = AiProvider::Custom;
        settings.ai.models = vec!["  google/gemma-3n-e4b  ".into()];
        settings.ai.custom_base_url = Some("http://127.0.0.1:1234/v1/".into());
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.models, vec!["google/gemma-3n-e4b".to_owned()]);
        assert_eq!(
            settings.ai.custom_base_url.as_deref(),
            Some("http://127.0.0.1:1234/v1")
        );
        // Junk base URLs collapse to None (Ollama default applies).
        let mut settings = AppSettings::default();
        settings.ai.provider = AiProvider::Custom;
        settings.ai.custom_base_url = Some("notaurl".into());
        let settings = settings.validate_and_normalize().unwrap();
        assert_eq!(settings.ai.custom_base_url, None);
    }
}
