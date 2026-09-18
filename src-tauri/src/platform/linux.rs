//! Linux test-bench adapter.
//!
//! One shared `AppCore` runs on every OS; this file is the entire Linux
//! difference. macOS owns Accessibility/SpeechAnalyzer, Windows owns
//! UIA/SAPI — Linux owns *testability*: a file-backed credential vault, an
//! in-process text buffer, and a fake speech session that still travels the
//! real `PlatformSpeechEngine` path (oneshot startup, `Final` on stop,
//! silence on cancel, `AudioLevel` meter).
//!
//! Deliberately dependency-free (std only) so `bun tauri dev` works on a
//! stock Ubuntu box with only the Tauri system prerequisites. Real clipboard
//! audio (PipeWire/Portal) is a future upgrade; the contract surface stays
//! identical, so swapping the inside of any method here cannot break
//! macOS/Windows.
//!
//! Environment overrides for deterministic tests:
//! - `KIVO_LINUX_TEST_TEXT`: text returned by `get_selected_text`.
//! - `KIVO_LINUX_DICTATION_TEXT`: transcript delivered by `stop()`.
//! - `KIVO_LINUX_CREDENTIAL_FILE`: credential vault path (tests point it at
//!   a temp file so they never touch `$HOME`).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::security::{CredentialError, CredentialStore, SecretString};

use super::*;

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

pub(super) struct PlatformImpl {
    credentials: LinuxCredentialStore,
    selection: Mutex<Option<(u64, String)>>,
}

impl PlatformImpl {
    pub(super) fn new() -> PlatformResult<Self> {
        Ok(Self {
            credentials: LinuxCredentialStore,
            selection: Mutex::new(None),
        })
    }

    pub(super) fn credential_store(&self) -> &dyn CredentialStore {
        &self.credentials
    }

    pub(super) fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        let text = std::env::var("KIVO_LINUX_TEST_TEXT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Hello, how are you?".to_owned());
        if text.trim().is_empty() {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "get_selected_text",
                "Select some text first.",
            ));
        }
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        *self
            .selection
            .lock()
            .map_err(|_| os_error("get_selected_text"))? = Some((token, text.clone()));
        Ok(SelectionSnapshot {
            text,
            bounds: vec![crate::platform::ScreenRect {
                x: 480.0,
                y: 320.0,
                width: 164.0,
                height: 22.0,
            }],
            owner: ActiveApplication {
                process_id: std::process::id(),
                name: "Kivo Linux Test Pad".to_owned(),
                identifier: Some("com.kivo.desktop.linux-test-pad".to_owned()),
                native_handle: std::process::id() as usize,
            },
            strategy: crate::text::TextAccessStrategy::Accessibility,
            native_token: token,
        })
    }

    pub(super) fn cursor_position(&self) -> PlatformResult<ScreenPoint> {
        Ok(ScreenPoint { x: 640.0, y: 360.0 })
    }

    pub(super) fn replace_selected_text(
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
        let mut selection = self
            .selection
            .lock()
            .map_err(|_| os_error("replace_selected_text"))?;
        match selection.as_ref() {
            Some((token, _)) if *token == snapshot.native_token => {
                *selection = None;
                Ok(())
            }
            _ => Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The original text field or selection changed.",
            )),
        }
    }

    pub(super) fn capture_insertion_target(
        &self,
    ) -> PlatformResult<Box<dyn crate::text::InsertionTarget>> {
        Ok(Box::new(LinuxInsertionTarget))
    }

    pub(super) fn permission_status(
        &self,
        permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        // No OS consent prompt exists on the bench: report the checkable
        // state instead of parking the UI on an Allow button that can never
        // resolve (same rationale as the Windows adapter).
        Ok(match permission {
            PermissionKind::Accessibility => PermissionStatus::Granted,
            PermissionKind::Microphone => PermissionStatus::Granted,
            PermissionKind::SpeechRecognition => PermissionStatus::Granted,
            PermissionKind::InputMonitoring => PermissionStatus::Unavailable,
        })
    }

    pub(super) fn request_permission(&self, permission: PermissionKind) -> PlatformResult<()> {
        match permission {
            PermissionKind::Accessibility
            | PermissionKind::Microphone
            | PermissionKind::SpeechRecognition => Ok(()),
            PermissionKind::InputMonitoring => Err(PlatformError::unsupported(
                "request_permission",
                "Input monitoring is only available on macOS.",
            )),
        }
    }

    pub(super) fn register_dictation_shortcut(
        &self,
        _shortcut: HoldShortcut,
        _callback: HoldShortcutCallback,
    ) -> PlatformResult<Box<dyn ShortcutRegistration>> {
        // Linux dictation always uses the portable global-shortcut plugin
        // (`Control+Alt+Space`); there is no native hold monitor to register.
        Err(PlatformError::unsupported(
            "register_dictation_shortcut",
            "Linux uses the portable dictation shortcut.",
        ))
    }

    pub(super) fn style_window(
        &self,
        _native_window: usize,
        _kind: OverlayKind,
    ) -> PlatformResult<()> {
        Ok(())
    }

    pub(super) fn start_speech(
        &self,
        _options: SpeechOptions,
        callback: SpeechCallback,
    ) -> PlatformResult<Box<dyn SpeechSession>> {
        // Announce Listening synchronously: `PlatformSpeechEngine` waits on a
        // oneshot for exactly this before reporting the session as live.
        callback(SpeechEvent::Listening);
        let stopped = Arc::new(AtomicBool::new(false));
        let meter_stop = Arc::clone(&stopped);
        let meter_callback = Arc::clone(&callback);
        std::thread::spawn(move || {
            // Honest fake meter: a few decaying levels, then silence. The UI
            // shows a recording label driven by these; no fabricated waveform.
            for level in [0.35, 0.55, 0.42, 0.3] {
                if meter_stop.load(Ordering::Relaxed) {
                    break;
                }
                meter_callback(SpeechEvent::AudioLevel(level));
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        });
        Ok(Box::new(LinuxSpeechSession {
            callback,
            stopped,
            cancelled: Arc::new(AtomicBool::new(false)),
        }))
    }
}

fn os_error(operation: &'static str) -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Os,
        operation,
        "Linux test-bench state is unavailable.",
    )
}

struct LinuxInsertionTarget;

impl crate::text::InsertionTarget for LinuxInsertionTarget {
    fn insert(&self, text: &str) -> Result<(), crate::text::TextError> {
        if text.is_empty() {
            return Err(crate::text::TextError::InsertionFailed);
        }
        Ok(())
    }
}

struct LinuxSpeechSession {
    callback: SpeechCallback,
    stopped: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl SpeechSession for LinuxSpeechSession {
    fn stop(&mut self) -> PlatformResult<()> {
        self.stopped.store(true, Ordering::Relaxed);
        if self.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let text = std::env::var("KIVO_LINUX_DICTATION_TEXT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Hello from the Linux test bench".to_owned());
        (self.callback)(SpeechEvent::Final(text));
        Ok(())
    }

    fn cancel(&mut self) -> PlatformResult<()> {
        self.cancelled.store(true, Ordering::Relaxed);
        self.stopped.store(true, Ordering::Relaxed);
        Ok(())
    }
}

struct LinuxCredentialStore;

impl LinuxCredentialStore {
    fn path() -> PathBuf {
        if let Ok(override_path) = std::env::var("KIVO_LINUX_CREDENTIAL_FILE")
            && !override_path.trim().is_empty()
        {
            return PathBuf::from(override_path);
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .filter(|v| !v.is_empty())
                    .map(|home| PathBuf::from(home).join(".config"))
            });
        base.map(|dir| dir.join("kivo").join("linux-credentials.json"))
            .unwrap_or_else(|| PathBuf::from("/tmp/kivo-linux-credentials.json"))
    }

    fn read_all(&self) -> HashMap<String, String> {
        let path = Self::path();
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<HashMap<String, String>>(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_all(&self, map: &HashMap<String, String>) -> Result<(), CredentialError> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| CredentialError::Backend)?;
        }
        let bytes = serde_json::to_vec(map).map_err(|_| CredentialError::Backend)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true).mode(0o600);
            use std::io::Write;
            let mut file = options.open(&path).map_err(|_| CredentialError::Backend)?;
            file.write_all(&bytes)
                .map_err(|_| CredentialError::Backend)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&path, bytes).map_err(|_| CredentialError::Backend)
        }
    }
}

impl CredentialStore for LinuxCredentialStore {
    fn save_api_key(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError> {
        // Reach into the secret without cloning it anywhere durable: the
        // serialized vault holds the key bytes only for this dev bench.
        // Shipping Linux would move this to Secret Service via `keyring`.
        let mut map = self.read_all();
        map.insert(account.to_owned(), secret.expose().to_owned());
        self.write_all(&map)
    }

    fn load_api_key(&self, account: &str) -> Result<Option<SecretString>, CredentialError> {
        let value = self.read_all().remove(account);
        value
            .map(SecretString::new)
            .transpose()
            .map_err(|_| CredentialError::Backend)
    }

    fn clear_api_key(&self, account: &str) -> Result<(), CredentialError> {
        let mut map = self.read_all();
        if map.remove(account).is_none() {
            return Ok(());
        }
        self.write_all(&map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::CredentialStore;

    fn temp_vault(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kivo-linux-creds-{}-{}.json",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // SAFETY: tests run single-threaded per binary by default for env
        // mutation; each test uses a unique file so parallel runs cannot
        // collide.
        unsafe {
            std::env::set_var("KIVO_LINUX_CREDENTIAL_FILE", &path);
        }
        path
    }

    fn clear_vault_env() {
        unsafe {
            std::env::remove_var("KIVO_LINUX_CREDENTIAL_FILE");
        }
    }

    #[test]
    fn linux_selection_round_trips_through_replace() {
        let platform = PlatformImpl::new().unwrap();
        let snapshot = platform.get_selected_text().unwrap();
        assert!(!snapshot.text.trim().is_empty());
        assert_eq!(snapshot.owner.name, "Kivo Linux Test Pad");
        platform
            .replace_selected_text(&snapshot, "replaced")
            .unwrap();
        // Token is single-use: a second replace with the same snapshot is a
        // stale-target refusal, never an unconditional paste.
        let stale = platform.replace_selected_text(&snapshot, "again");
        assert!(matches!(
            stale,
            Err(error) if error.kind == PlatformErrorKind::InvalidState
        ));
    }

    #[test]
    fn linux_permissions_need_no_prompt() {
        let platform = PlatformImpl::new().unwrap();
        assert_eq!(
            platform
                .permission_status(PermissionKind::Accessibility)
                .unwrap(),
            PermissionStatus::Granted
        );
        assert_eq!(
            platform
                .permission_status(PermissionKind::Microphone)
                .unwrap(),
            PermissionStatus::Granted
        );
        assert_eq!(
            platform
                .permission_status(PermissionKind::SpeechRecognition)
                .unwrap(),
            PermissionStatus::Granted
        );
        assert_eq!(
            platform
                .permission_status(PermissionKind::InputMonitoring)
                .unwrap(),
            PermissionStatus::Unavailable
        );
        assert!(
            platform
                .request_permission(PermissionKind::Microphone)
                .is_ok()
        );
        assert!(
            platform
                .request_permission(PermissionKind::InputMonitoring)
                .is_err()
        );
    }

    #[test]
    fn linux_speech_delivers_a_final_on_stop_and_silence_on_cancel() {
        let platform = PlatformImpl::new().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_for_callback = Arc::clone(&events);
        let mut session = platform
            .start_speech(
                SpeechOptions {
                    language: None,
                    microphone_id: None,
                    require_on_device: false,
                },
                Arc::new(move |event| {
                    let label = match &event {
                        SpeechEvent::Listening => "listening".to_owned(),
                        SpeechEvent::Final(text) => format!("final:{text}"),
                        SpeechEvent::AudioLevel(_) => "level".to_owned(),
                        SpeechEvent::Partial(text) => format!("partial:{text}"),
                        SpeechEvent::Error(_) => "error".to_owned(),
                    };
                    events_for_callback.lock().unwrap().push(label);
                }),
            )
            .unwrap();
        // meter thread needs a moment; Listening was synchronous.
        std::thread::sleep(std::time::Duration::from_millis(60));
        session.stop().unwrap();
        let events = events.lock().unwrap();
        assert!(events.iter().any(|e| e == "listening"));
        assert!(events.iter().any(|e| e.starts_with("final:")));

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_for_callback = Arc::clone(&events);
        let mut session = platform
            .start_speech(
                SpeechOptions {
                    language: None,
                    microphone_id: None,
                    require_on_device: false,
                },
                Arc::new(move |event| {
                    if let SpeechEvent::Final(text) = &event {
                        events_for_callback
                            .lock()
                            .unwrap()
                            .push(format!("final:{text}"));
                    }
                }),
            )
            .unwrap();
        session.cancel().unwrap();
        session.stop().unwrap();
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn linux_credentials_round_trip_in_an_isolated_vault() {
        let path = temp_vault("roundtrip");
        let store = LinuxCredentialStore;
        assert!(store.load_api_key("gemini").unwrap().is_none());
        store
            .save_api_key(
                "gemini",
                &SecretString::new("  test-key-123  ".into()).unwrap(),
            )
            .unwrap();
        assert!(store.status("gemini").unwrap().configured);
        // A fresh handle reads the same file: persistence across restarts.
        assert!(LinuxCredentialStore.status("gemini").unwrap().configured);
        store.clear_api_key("gemini").unwrap();
        assert!(!LinuxCredentialStore.status("gemini").unwrap().configured);
        let _ = std::fs::remove_file(&path);
        clear_vault_env();
    }
}
