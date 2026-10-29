//! Platform integration points. The Tauri build implements these with
//! Accessibility/UIA, SAPI/SpeechAnalyzer, Keychain/Credential Manager.
//! The one-go native port keeps the same trait surface; OS capture/insertion
//! and live speech engines are stubbed to explicit Unsupported errors so no
//! caller silently believes text was inserted.
//! shortcut: live AX/UIA capture + OS speech engines not yet ported; wire `platform::macos`
//! / `platform::windows` back up when targeting those OSes.

#[derive(Clone, Debug)]
pub struct Selection {
    pub text: String,
    pub app_name: String,
    pub can_replace: bool,
}

#[derive(Clone, Debug)]
pub enum PlatformError {
    Unsupported(&'static str),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(what) => {
                write!(f, "{what} isn't available in this native build yet.")
            }
        }
    }
}

pub fn capture_selection() -> Result<Selection, PlatformError> {
    Err(PlatformError::Unsupported("System selection capture"))
}

pub fn replace_selection(_text: &str) -> Result<bool, PlatformError> {
    Err(PlatformError::Unsupported("System text replacement"))
}

pub fn copy_to_clipboard(ctx: &egui::Context, text: &str) {
    ctx.copy_text(text.to_owned());
}

pub fn microphones() -> Vec<(String, String)> {
    vec![("default".to_string(), "System default".to_string())]
}

pub fn speech_languages() -> Vec<(String, String)> {
    vec![
        ("auto".to_string(), "Automatic".to_string()),
        ("en-US".to_string(), "English (US)".to_string()),
        ("en-GB".to_string(), "English (UK)".to_string()),
    ]
}
