use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub theme: Theme,
    pub launch_at_login: bool,
    pub start_in_background: bool,
    pub show_idle_flow_bar: bool,
    pub onboarding_complete: bool,
    pub dictation_shortcut: String,
    pub writing_shortcut: String,
    pub microphone_id: Option<String>,
    pub dictation_language: String,
    pub improve_dictation_with_ai: bool,
    pub sound_feedback: bool,
    pub ai_model: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: Theme::System,
            launch_at_login: false,
            start_in_background: true,
            show_idle_flow_bar: false,
            onboarding_complete: false,
            dictation_shortcut: "Ctrl+Meta".to_string(),
            writing_shortcut: "Ctrl+Space".to_string(),
            microphone_id: None,
            dictation_language: "auto".to_string(),
            improve_dictation_with_ai: false,
            sound_feedback: true,
            ai_model: crate::ai::MODEL.to_string(),
        }
    }
}

impl Settings {
    pub fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("kivo").join("settings.json"))
    }

    pub fn load() -> Self {
        let path = match Self::path() {
            Some(p) => p,
            None => return Self::default(),
        };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.trim().is_empty() {
            return Self::default();
        }
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or_else(|| "No config dir".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Atomic write: temp file + rename so a crash never leaves half-written JSON.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn older_settings_get_a_default_model() {
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        json.as_object_mut().unwrap().remove("aiModel");
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.ai_model, crate::ai::MODEL);
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings::default();
        std::fs::write(&path, serde_json::to_string_pretty(&s).unwrap()).unwrap();
        let back: Settings =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(s, back);
    }
}
