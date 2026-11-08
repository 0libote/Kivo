//! Platform integration for the native app.
//!
//! Clipboard capture/replacement is real (OS clipboard via `arboard` +
//! egui). Microphone enumeration, global shortcuts, tray, autostart and
//! live speech engines are ported incrementally; anything not yet wired
//! returns an explicit `Unsupported` error so no caller silently believes
//! text was inserted or audio was transcribed.

#[derive(Clone, Debug)]
pub struct Selection {
    pub text: String,
    pub app_name: String,
    pub can_replace: bool,
}

#[derive(Clone, Debug)]
pub enum PlatformError {
    #[allow(dead_code)]
    Unsupported(&'static str),
    NotFound(String),
    Backend(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(what) => {
                write!(f, "{what} isn't available in this build yet.")
            }
            Self::NotFound(msg) => f.write_str(msg),
            Self::Backend(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Read the current clipboard as the "captured selection".
///
/// Real flow (same on Windows/Mac/Linux): the user copies text in any app,
/// then presses Capture. This reads the OS clipboard so Writing Tools works
/// today without Accessibility/UIA grants.
pub fn capture_selection() -> Result<Selection, PlatformError> {
    // Test hook: deterministic clipboard without a display server.
    if let Ok(test) = std::env::var("KIVO_TEST_CLIPBOARD") {
        if test.trim().is_empty() {
            return Err(PlatformError::NotFound(
                "Copy some text first, then choose Capture.".to_string(),
            ));
        }
        return Ok(Selection {
            text: test,
            app_name: "Test clipboard".to_string(),
            can_replace: true,
        });
    }
    let mut clipboard = arboard::Clipboard::new().map_err(|_| {
        PlatformError::NotFound(
            "Clipboard isn't available. Copy some text first, then choose Capture.".to_string(),
        )
    })?;
    let text = clipboard.get_text().map_err(|_| {
        PlatformError::NotFound("Copy some text first, then choose Capture.".to_string())
    })?;
    if text.trim().is_empty() {
        return Err(PlatformError::NotFound(
            "Copy some text first, then choose Capture.".to_string(),
        ));
    }
    if text.chars().count() > 200_000 {
        return Err(PlatformError::Backend(
            "That selection is too long (200,000 char limit).".to_string(),
        ));
    }
    Ok(Selection {
        text,
        app_name: "Clipboard".to_string(),
        can_replace: true,
    })
}

/// Replace = write the result to the clipboard so the user can paste.
/// Direct keystroke injection (Ctrl+V simulation) ships with the global
/// shortcut integration; until then this never pretends to have typed.
pub fn replace_selection(text: &str) -> Result<bool, PlatformError> {
    if text.trim().is_empty() {
        return Err(PlatformError::Backend(
            "Nothing to replace with.".to_string(),
        ));
    }
    if let Ok(test_path) = std::env::var("KIVO_TEST_CLIPBOARD_OUT") {
        std::fs::write(test_path, text)
            .map_err(|_| PlatformError::Backend("Could not write the replacement.".to_string()))?;
        return Ok(true);
    }
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|_| PlatformError::Backend("Clipboard isn't available.".to_string()))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|_| PlatformError::Backend("Could not write to the clipboard.".to_string()))?;
    Ok(true)
}

pub fn copy_to_clipboard(ctx: &egui::Context, text: &str) {
    ctx.copy_text(text.to_owned());
    // Belt and suspenders: egui owns the clipboard while focused; arboard
    // covers headless/paste-after-quit paths. Best effort only.
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text.to_owned());
    }
}

#[allow(dead_code)]
pub fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub fn microphones() -> Vec<(String, String)> {
    // Real enumeration (cpal) lands with the dictation audio work; until then
    // report the honest single default instead of fabricating devices.
    vec![("default".to_string(), "System default".to_string())]
}

pub fn speech_languages() -> Vec<(String, String)> {
    vec![
        ("auto".to_string(), "Automatic".to_string()),
        ("en-US".to_string(), "English (US)".to_string()),
        ("en-GB".to_string(), "English (UK)".to_string()),
        ("de-DE".to_string(), "Deutsch".to_string()),
        ("fr-FR".to_string(), "Français".to_string()),
        ("es-ES".to_string(), "Español".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_uses_test_clipboard_without_display() {
        // SAFETY: isolated env mutation for this test binary.
        unsafe { std::env::set_var("KIVO_TEST_CLIPBOARD", "hello") };
        let sel = capture_selection().unwrap();
        assert_eq!(sel.text, "hello");
        assert!(sel.can_replace);
        unsafe { std::env::set_var("KIVO_TEST_CLIPBOARD", "   ") };
        assert!(capture_selection().is_err());
        unsafe { std::env::remove_var("KIVO_TEST_CLIPBOARD") };
    }

    #[test]
    fn replace_writes_test_output_without_display() {
        let dir = std::env::temp_dir().join(format!(
            "kivo-replace-{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // SAFETY: isolated env mutation for this test binary.
        unsafe { std::env::set_var("KIVO_TEST_CLIPBOARD_OUT", &dir) };
        assert!(replace_selection("  ").is_err());
        assert!(replace_selection("new text").unwrap());
        assert_eq!(std::fs::read_to_string(&dir).unwrap(), "new text");
        let _ = std::fs::remove_file(&dir);
        unsafe { std::env::remove_var("KIVO_TEST_CLIPBOARD_OUT") };
    }
}
