use std::{fmt, sync::Arc};

use serde::Serialize;

use crate::security::CredentialStore;

pub(crate) mod adapters;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos::PlatformImpl;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use unsupported::PlatformImpl;
#[cfg(target_os = "windows")]
use windows::PlatformImpl;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformErrorKind {
    Unsupported,
    PermissionDenied,
    NotFound,
    InvalidState,
    ShortcutConflict,
    Speech,
    Os,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformError {
    pub kind: PlatformErrorKind,
    pub operation: &'static str,
    pub message: String,
    pub os_code: Option<i64>,
}

impl PlatformError {
    pub(crate) fn new(
        kind: PlatformErrorKind,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation,
            message: message.into(),
            os_code: None,
        }
    }

    pub(crate) fn with_os_code(mut self, code: i64) -> Self {
        self.os_code = Some(code);
        self
    }

    pub(crate) fn unsupported(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(PlatformErrorKind::Unsupported, operation, detail)
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PlatformError {}

pub type PlatformResult<T> = Result<T, PlatformError>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub fn anchor_below(self) -> ScreenPoint {
        ScreenPoint {
            x: self.x + self.width / 2.0,
            y: self.y + self.height,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveApplication {
    pub process_id: u32,
    pub name: String,
    pub identifier: Option<String>,
    /// HWND on Windows and the owning process id on macOS. It is never sent back
    /// by the frontend and is only used to correlate a selection snapshot.
    #[serde(skip)]
    pub native_handle: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSnapshot {
    pub text: String,
    pub bounds: Vec<ScreenRect>,
    pub owner: ActiveApplication,
    #[serde(skip)]
    pub(crate) native_token: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    Accessibility,
    Microphone,
    SpeechRecognition,
    InputMonitoring,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Restricted,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierKey {
    #[cfg(target_os = "macos")]
    Function,
    #[cfg(target_os = "windows")]
    Control,
    #[cfg(target_os = "windows")]
    Meta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldShortcut {
    pub modifiers: Vec<ModifierKey>,
    /// Optional native virtual-key code. `None` denotes a modifier-only hold.
    pub key_code: Option<u32>,
    pub suppress: bool,
}

impl HoldShortcut {
    pub fn platform_default() -> Self {
        #[cfg(target_os = "macos")]
        return Self {
            modifiers: vec![ModifierKey::Function],
            key_code: None,
            suppress: true,
        };

        #[cfg(target_os = "windows")]
        return Self {
            modifiers: vec![ModifierKey::Control, ModifierKey::Meta],
            key_code: None,
            suppress: true,
        };

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Self {
            modifiers: Vec::new(),
            key_code: None,
            suppress: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldShortcutEvent {
    Pressed,
    Released,
    Cancelled,
}

pub type HoldShortcutCallback = Arc<dyn Fn(HoldShortcutEvent) + Send + Sync + 'static>;

pub trait ShortcutRegistration: Send {
    fn stop(&mut self) -> PlatformResult<()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayKind {
    FlowBar,
    WritingTools,
    Settings,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpeechOptions {
    pub language: Option<String>,
    pub microphone_id: Option<String>,
    pub require_on_device: bool,
}

#[derive(Clone, Debug)]
pub enum SpeechEvent {
    Listening,
    Partial(String),
    Final(String),
    AudioLevel(f32),
    Error(PlatformError),
}

pub type SpeechCallback = Arc<dyn Fn(SpeechEvent) + Send + Sync + 'static>;

pub trait SpeechSession: Send {
    fn stop(&mut self) -> PlatformResult<()>;
    fn cancel(&mut self) -> PlatformResult<()>;
}

pub struct PlatformServices {
    implementation: PlatformImpl,
}

impl PlatformServices {
    pub fn new() -> PlatformResult<Self> {
        Ok(Self {
            implementation: PlatformImpl::new()?,
        })
    }

    pub fn credential_store(&self) -> &dyn CredentialStore {
        self.implementation.credential_store()
    }

    pub fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        self.implementation.get_selected_text()
    }

    pub fn replace_selected_text(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> PlatformResult<()> {
        if replacement.is_empty() {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "Replacement text is empty.",
            ));
        }
        self.implementation
            .replace_selected_text(snapshot, replacement)
    }

    pub fn insert_text_at_cursor(&self, text: &str) -> PlatformResult<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.implementation.insert_text_at_cursor(text)
    }

    pub fn get_cursor_or_selection_position(&self) -> PlatformResult<ScreenPoint> {
        self.implementation.get_cursor_or_selection_position()
    }

    pub fn permission_status(
        &self,
        permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        self.implementation.permission_status(permission)
    }

    /// Initiates the native permission request. Call `permission_status` again
    /// after the operating system completes its prompt.
    pub fn request_permission(&self, permission: PermissionKind) -> PlatformResult<()> {
        self.implementation.request_permission(permission)
    }

    pub fn register_dictation_shortcut(
        &self,
        shortcut: HoldShortcut,
        callback: HoldShortcutCallback,
    ) -> PlatformResult<Box<dyn ShortcutRegistration>> {
        self.implementation
            .register_dictation_shortcut(shortcut, callback)
    }

    /// Applies native chrome/material behavior to an existing Tauri native
    /// window. The handle is `NSWindow*` on macOS and `HWND` on Windows.
    pub fn style_window(&self, native_window: usize, kind: OverlayKind) -> PlatformResult<()> {
        if native_window == 0 {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "style_window",
                "The native window handle is invalid.",
            ));
        }
        self.implementation.style_window(native_window, kind)
    }

    pub fn start_speech(
        &self,
        options: SpeechOptions,
        callback: SpeechCallback,
    ) -> PlatformResult<Box<dyn SpeechSession>> {
        self.implementation.start_speech(options, callback)
    }
}
