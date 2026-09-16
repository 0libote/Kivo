use std::{
    collections::HashSet,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::ai::WritingAction;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

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
    Other,
}

impl HostPlatform {
    pub(crate) fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Self::Macos;
        #[cfg(target_os = "windows")]
        return Self::Windows;
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Self::Other;
    }
}

/// Native hold shortcut per host, parameterized so every CI platform can
/// assert every other platform's default (a `#[cfg]`-gated test only ever
/// exercises its own host and lets the other default drift silently).
pub(crate) fn dictation_default_for(host: HostPlatform) -> ShortcutBinding {
    match host {
        HostPlatform::Macos => ShortcutBinding::new("Fn"),
        HostPlatform::Windows => ShortcutBinding::new("Ctrl+Meta"),
        HostPlatform::Other => ShortcutBinding::new("Control+Alt+Space"),
    }
}

/// Portable Writing Tools shortcut per host. Same cross-host testability
/// rationale as [`dictation_default_for`].
pub(crate) fn writing_tools_default_for(host: HostPlatform) -> ShortcutBinding {
    match host {
        HostPlatform::Macos => ShortcutBinding::new("Ctrl+Shift+Space"),
        HostPlatform::Windows => ShortcutBinding::new("Ctrl+Space"),
        HostPlatform::Other => ShortcutBinding::new("Control+Alt+Space"),
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
    pub model: String,
    #[serde(default)]
    pub backup_model: Option<String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            model: crate::ai::DEFAULT_GEMINI_MODEL.to_owned(),
            backup_model: None,
        }
    }
}

impl AiSettings {
    fn normalize(&mut self) {
        self.model = crate::ai::normalize_model(&self.model);
        self.backup_model =
            crate::ai::normalize_backup_model(self.backup_model.as_deref(), &self.model);
    }

    fn validate(&self) -> Result<(), SettingsError> {
        if !crate::ai::is_usable_model(&self.model) {
            return Err(SettingsError::InvalidAiModel);
        }
        if let Some(backup) = &self.backup_model
            && (!crate::ai::is_usable_model(backup)
                || crate::ai::canonical_model_id(backup)
                    == crate::ai::canonical_model_id(&self.model))
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
            writing_tools_default_for(HostPlatform::Macos),
            ShortcutBinding::new("Ctrl+Shift+Space")
        );
        assert_eq!(
            writing_tools_default_for(HostPlatform::Windows),
            ShortcutBinding::new("Ctrl+Space")
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
            AppSettings::default().ai.model,
            crate::ai::DEFAULT_GEMINI_MODEL
        );
        assert_eq!(AppSettings::default().ai.backup_model, None);

        // Supported ids survive normalization (trimmed), including new ids
        // that were never in the curated suggestion list.
        let mut settings = AppSettings::default();
        settings.ai.model = "  gemini-2.5-flash  ".into();
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.model,
            "gemini-2.5-flash"
        );
        let mut settings = AppSettings::default();
        settings.ai.model = "models/gemini-4.0-flash".into();
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.model,
            "gemini-4.0-flash"
        );

        // Unknown, TTS/image, and empty ids fall back to the default instead
        // of bricking AI requests.
        for model in [
            "",
            "has spaces!",
            "gemini-2.5-flash-preview-tts",
            "gemini-2.5-flash-image",
        ] {
            let mut settings = AppSettings::default();
            settings.ai.model = model.into();
            assert_eq!(
                settings.validate_and_normalize().unwrap().ai.model,
                crate::ai::DEFAULT_GEMINI_MODEL,
                "model {model} should fall back"
            );
        }

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
        assert_eq!(settings.ai.model, crate::ai::DEFAULT_GEMINI_MODEL);
        assert_eq!(settings.ai.backup_model, None);
    }

    #[test]
    fn ai_backup_model_normalizes_to_none_when_empty_same_or_unusable() {
        // A distinct usable backup survives.
        let mut settings = AppSettings::default();
        settings.ai.backup_model = Some("gemini-2.5-flash".into());
        assert_eq!(
            settings.validate_and_normalize().unwrap().ai.backup_model,
            Some("gemini-2.5-flash".to_owned())
        );
        // Empty, same-as-primary, and blocked ids collapse to None.
        for backup in [
            Some(""),
            Some("  "),
            Some("gemini-3.8-flash"),
            Some("gemini-2.5-flash-preview-tts"),
            Some("has spaces!"),
        ] {
            let mut settings = AppSettings::default();
            settings.ai.backup_model = backup.map(str::to_owned);
            assert_eq!(
                settings.validate_and_normalize().unwrap().ai.backup_model,
                None,
                "backup {backup:?} should collapse"
            );
        }
    }
}
