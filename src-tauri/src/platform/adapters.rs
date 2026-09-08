use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::{
    security::{CredentialError, CredentialStatus, CredentialStore, SecretString},
    speech::{
        MicrophoneDevice, SpeechEngine, SpeechError, SpeechEvent as CoreSpeechEvent,
        SpeechEventSink, SpeechFuture, SpeechSessionId, SpeechStartOptions, SpeechTranscript,
    },
    text::{
        ActiveApplication as CoreApplication, CapturedSelection, ScreenRect as CoreRect,
        TextAccessStrategy, TextError, TextFuture, TextService,
    },
};

use super::{
    PlatformError, PlatformErrorKind, PlatformServices, SelectionSnapshot, SpeechEvent,
    SpeechOptions, SpeechSession,
};

pub(crate) struct PlatformCredentialStore {
    platform: Arc<PlatformServices>,
}

impl PlatformCredentialStore {
    pub(crate) fn new(platform: Arc<PlatformServices>) -> Self {
        Self { platform }
    }
}

impl CredentialStore for PlatformCredentialStore {
    fn save_api_key(&self, secret: &SecretString) -> Result<(), CredentialError> {
        self.platform.credential_store().save_api_key(secret)
    }

    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError> {
        self.platform.credential_store().load_api_key()
    }

    fn clear_api_key(&self) -> Result<(), CredentialError> {
        self.platform.credential_store().clear_api_key()
    }

    fn status(&self) -> Result<CredentialStatus, CredentialError> {
        self.platform.credential_store().status()
    }
}

pub(crate) struct PlatformTextService {
    platform: Arc<PlatformServices>,
    next_ticket: AtomicU64,
    selections: Mutex<HashMap<u64, SelectionSnapshot>>,
}

impl PlatformTextService {
    pub(crate) fn new(platform: Arc<PlatformServices>) -> Self {
        Self {
            platform,
            next_ticket: AtomicU64::new(1),
            selections: Mutex::new(HashMap::new()),
        }
    }
}

impl TextService for PlatformTextService {
    fn capture_selection(&self) -> TextFuture<'_, Result<CapturedSelection, TextError>> {
        Box::pin(async move {
            let snapshot = self
                .platform
                .get_selected_text()
                .map_err(text_error_from_platform)?;
            let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
            let application = CoreApplication {
                identifier: snapshot
                    .owner
                    .identifier
                    .clone()
                    .unwrap_or_else(|| snapshot.owner.process_id.to_string()),
                display_name: snapshot.owner.name.clone(),
            };
            let anchor = snapshot.bounds.last().map(|bounds| CoreRect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
            });
            let selection = CapturedSelection::new(
                ticket,
                snapshot.text.clone(),
                application,
                anchor,
                TextAccessStrategy::Accessibility,
            )?;
            self.selections
                .lock()
                .map_err(|_| TextError::Backend)?
                .insert(ticket, snapshot);
            Ok(selection)
        })
    }

    fn replace_selected_text<'a>(
        &'a self,
        selection: &'a CapturedSelection,
        replacement: &'a str,
    ) -> TextFuture<'a, Result<(), TextError>> {
        Box::pin(async move {
            if selection.strategy() != TextAccessStrategy::Accessibility {
                return Err(TextError::UnsupportedApplication);
            }
            let snapshot = self
                .selections
                .lock()
                .map_err(|_| TextError::Backend)?
                .remove(&selection.ticket())
                .ok_or(TextError::SelectionExpired)?;
            self.platform
                .replace_selected_text(&snapshot, replacement)
                .map_err(text_error_from_platform)
        })
    }

    fn insert_text_at_cursor<'a>(&'a self, text: &'a str) -> TextFuture<'a, Result<(), TextError>> {
        Box::pin(async move {
            self.platform
                .insert_text_at_cursor(text)
                .map_err(text_error_from_platform)
        })
    }
}

struct NativeSpeechSession {
    session: Box<dyn SpeechSession>,
    final_result: Arc<Mutex<Option<Result<String, SpeechError>>>>,
}

pub(crate) struct PlatformSpeechEngine {
    platform: Arc<PlatformServices>,
    next_session: AtomicU64,
    sessions: Mutex<HashMap<SpeechSessionId, NativeSpeechSession>>,
}

impl PlatformSpeechEngine {
    pub(crate) fn new(platform: Arc<PlatformServices>) -> Self {
        Self {
            platform,
            next_session: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl SpeechEngine for PlatformSpeechEngine {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        Box::pin(async {
            Ok(vec![MicrophoneDevice {
                id: "default".into(),
                name: "System Default".into(),
                is_default: true,
            }])
        })
    }

    fn start(
        &self,
        options: SpeechStartOptions,
        events: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        Box::pin(async move {
            let id = SpeechSessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
            let final_result = Arc::new(Mutex::new(None));
            let result_for_callback = Arc::clone(&final_result);
            let session = self
                .platform
                .start_speech(
                    SpeechOptions {
                        language: options.locale,
                        microphone_id: options
                            .microphone_id
                            .filter(|microphone| microphone != "default"),
                        require_on_device: true,
                    },
                    Arc::new(move |event| match event {
                        SpeechEvent::AudioLevel(level) => {
                            events(CoreSpeechEvent::AudioLevel(level))
                        }
                        SpeechEvent::Listening => {
                            events(CoreSpeechEvent::SpeechDetected);
                        }
                        SpeechEvent::Partial(text) => {
                            if !text.trim().is_empty() {
                                events(CoreSpeechEvent::SpeechDetected);
                            }
                        }
                        SpeechEvent::Final(text) => {
                            if let Ok(mut result) = result_for_callback.lock() {
                                *result = Some(Ok(text));
                            }
                        }
                        SpeechEvent::Error(error) => {
                            if let Ok(mut result) = result_for_callback.lock() {
                                *result = Some(Err(speech_error_from_platform(error)));
                            }
                        }
                    }),
                )
                .map_err(speech_error_from_platform)?;
            self.sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .insert(
                    id,
                    NativeSpeechSession {
                        session,
                        final_result,
                    },
                );
            Ok(id)
        })
    }

    fn stop(
        &self,
        session_id: SpeechSessionId,
    ) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>> {
        Box::pin(async move {
            let final_result = {
                let mut sessions = self.sessions.lock().map_err(|_| SpeechError::Backend)?;
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or(SpeechError::NotRunning)?;
                session.session.stop().map_err(speech_error_from_platform)?;
                Arc::clone(&session.final_result)
            };

            let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
            loop {
                if let Some(result) = final_result
                    .lock()
                    .map_err(|_| SpeechError::Backend)?
                    .take()
                {
                    self.sessions
                        .lock()
                        .map_err(|_| SpeechError::Backend)?
                        .remove(&session_id);
                    return SpeechTranscript::new(result?);
                }
                if tokio::time::Instant::now() >= deadline {
                    self.sessions
                        .lock()
                        .map_err(|_| SpeechError::Backend)?
                        .remove(&session_id);
                    return Err(SpeechError::NoSpeechDetected);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
    }

    fn cancel(&self, session_id: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async move {
            let mut session = self
                .sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&session_id)
                .ok_or(SpeechError::NotRunning)?;
            session.session.cancel().map_err(speech_error_from_platform)
        })
    }
}

fn text_error_from_platform(error: PlatformError) -> TextError {
    match (error.kind, error.operation) {
        (PlatformErrorKind::PermissionDenied, _) => TextError::AccessibilityPermissionRequired,
        (PlatformErrorKind::NotFound, "get_selected_text") => TextError::NoSelection,
        (PlatformErrorKind::InvalidState, "replace_selected_text") => TextError::SelectionExpired,
        (PlatformErrorKind::Unsupported, "replace_selected_text") => TextError::ReplacementFailed,
        (PlatformErrorKind::Unsupported, _) => TextError::UnsupportedApplication,
        (_, "replace_selected_text") => TextError::ReplacementFailed,
        (_, "insert_text_at_cursor") => TextError::InsertionFailed,
        _ => TextError::Backend,
    }
}

fn speech_error_from_platform(error: PlatformError) -> SpeechError {
    match error.kind {
        PlatformErrorKind::PermissionDenied if error.operation.contains("microphone") => {
            SpeechError::MicrophonePermissionRequired
        }
        PlatformErrorKind::PermissionDenied => SpeechError::SpeechPermissionRequired,
        PlatformErrorKind::NotFound => SpeechError::MicrophoneUnavailable,
        PlatformErrorKind::Unsupported => SpeechError::RecognitionUnavailable,
        PlatformErrorKind::InvalidState => SpeechError::AlreadyRunning,
        PlatformErrorKind::Speech | PlatformErrorKind::Os => SpeechError::Backend,
        _ => SpeechError::Backend,
    }
}
