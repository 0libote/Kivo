use crate::security::{CredentialError, CredentialStore, SecretString};

use super::*;

pub(super) struct PlatformImpl {
    credentials: UnsupportedCredentialStore,
}

impl PlatformImpl {
    pub(super) fn new() -> PlatformResult<Self> {
        Ok(Self {
            credentials: UnsupportedCredentialStore,
        })
    }

    pub(super) fn credential_store(&self) -> &dyn CredentialStore {
        &self.credentials
    }

    pub(super) fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        Err(unsupported("get_selected_text"))
    }

    pub(super) fn replace_selected_text(
        &self,
        _snapshot: &SelectionSnapshot,
        _replacement: &str,
    ) -> PlatformResult<()> {
        Err(unsupported("replace_selected_text"))
    }

    pub(super) fn insert_text_at_cursor(&self, _text: &str) -> PlatformResult<()> {
        Err(unsupported("insert_text_at_cursor"))
    }

    pub(super) fn get_cursor_or_selection_position(&self) -> PlatformResult<ScreenPoint> {
        Err(unsupported("get_cursor_or_selection_position"))
    }

    pub(super) fn permission_status(
        &self,
        _permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        Ok(PermissionStatus::Unavailable)
    }

    pub(super) fn request_permission(&self, _permission: PermissionKind) -> PlatformResult<()> {
        Err(unsupported("request_permission"))
    }

    pub(super) fn register_dictation_shortcut(
        &self,
        _shortcut: HoldShortcut,
        _callback: HoldShortcutCallback,
    ) -> PlatformResult<Box<dyn ShortcutRegistration>> {
        Err(unsupported("register_dictation_shortcut"))
    }

    pub(super) fn style_window(
        &self,
        _native_window: usize,
        _kind: OverlayKind,
    ) -> PlatformResult<()> {
        Err(unsupported("style_window"))
    }

    pub(super) fn start_speech(
        &self,
        _options: SpeechOptions,
        _callback: SpeechCallback,
    ) -> PlatformResult<Box<dyn SpeechSession>> {
        Err(unsupported("start_speech"))
    }
}

struct UnsupportedCredentialStore;

impl CredentialStore for UnsupportedCredentialStore {
    fn save_api_key(&self, _secret: &SecretString) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn clear_api_key(&self) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }
}

fn unsupported(operation: &'static str) -> PlatformError {
    PlatformError::unsupported(operation, "This platform is not supported by Kivo.")
}
