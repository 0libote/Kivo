//! Desert Ant Voz behind Kivo's shared batch speech engine contract.
//!
//! Platform adapters provide model lifecycle and inference. Rust remains the
//! owner of the microphone, language gate, session ids, cancellation, and final
//! result. The adapter boundary receives only mono 16 kHz samples.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};
#[cfg(windows)]
use tauri::{AppHandle, Emitter};

use super::{
    MicrophoneDevice, SpeechBackend, SpeechEngine, SpeechError, SpeechEventSink, SpeechFuture,
    SpeechSessionId, SpeechStartOptions, SpeechTranscript, VozTranscript, capture::AudioCapture,
};

pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "hr", "hu", "it", "lt", "lv", "mt",
    "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "uk",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VozModelPhase {
    Unsupported,
    NotDownloaded,
    Downloading,
    Preparing,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VozModelStatus {
    pub supported: bool,
    pub downloaded: bool,
    pub phase: VozModelPhase,
    pub progress: Option<f64>,
    pub runtime: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(windows)]
pub struct VozWorkerRequest {
    pub request_id: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VozWorkerReply {
    pub request_id: String,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<VozModelPhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<VozTranscript>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn supports_language(locale: &str) -> bool {
    if locale.eq_ignore_ascii_case("auto") {
        return true;
    }
    let language = locale.split(['-', '_']).next().unwrap_or(locale);
    SUPPORTED_LANGUAGES.contains(&language.to_ascii_lowercase().as_str())
}

pub fn validate_detected_language(
    selected: &str,
    detected: Option<&str>,
    reliable: bool,
) -> Result<(), SpeechError> {
    let Some(detected) = detected.filter(|_| reliable) else {
        return Err(SpeechError::VozLanguageUncertain);
    };
    if !SUPPORTED_LANGUAGES.contains(&detected.to_ascii_lowercase().as_str()) {
        return Err(SpeechError::VozDetectedUnsupported(detected.to_owned()));
    }
    if selected.eq_ignore_ascii_case("auto") {
        return Ok(());
    }
    let selected_code = selected.split(['-', '_']).next().unwrap_or(selected);
    if selected_code.eq_ignore_ascii_case(detected) {
        Ok(())
    } else {
        Err(SpeechError::VozLanguageMismatch {
            selected: selected_code.to_owned(),
            detected: detected.to_owned(),
        })
    }
}

/// Runtime seam implemented by the macOS Swift SDK bridge and Windows web
/// worker adapter. Model downloads and preparation happen before capture.
pub trait VozRuntime: Send + Sync {
    fn model_status(&self) -> SpeechFuture<'_, Result<VozModelStatus, SpeechError>>;
    fn download(&self) -> SpeechFuture<'_, Result<(), SpeechError>>;
    fn remove_model(&self) -> SpeechFuture<'_, Result<(), SpeechError>>;
    fn set_worker_ready(&self, _ready: bool) -> Result<(), SpeechError> {
        Ok(())
    }
    fn complete_worker_request(&self, _reply: VozWorkerReply) -> Result<(), SpeechError> {
        Err(SpeechError::VozUnavailable)
    }
    fn prepare(&self) -> SpeechFuture<'_, Result<(), SpeechError>>;
    fn transcribe(
        &self,
        samples: Vec<f32>,
        locale: String,
    ) -> SpeechFuture<'_, Result<VozTranscript, SpeechError>>;
}

#[cfg(any(test, windows))]
#[derive(Clone)]
struct WorkerReadiness(tokio::sync::watch::Sender<bool>);

#[cfg(any(test, windows))]
impl WorkerReadiness {
    fn new() -> Self {
        Self(tokio::sync::watch::channel(false).0)
    }

    fn set(&self, ready: bool) {
        self.0.send_replace(ready);
    }

    async fn wait(&self, timeout: std::time::Duration) -> Result<(), SpeechError> {
        let mut ready = self.0.subscribe();
        if *ready.borrow_and_update() {
            return Ok(());
        }
        tokio::time::timeout(timeout, async {
            loop {
                ready
                    .changed()
                    .await
                    .map_err(|_| SpeechError::VozUnavailable)?;
                if *ready.borrow_and_update() {
                    return Ok(());
                }
            }
        })
        .await
        .map_err(|_| worker_not_ready_error())?
    }
}

#[cfg(any(test, windows))]
fn worker_not_ready_error() -> SpeechError {
    #[cfg(any(windows, target_os = "macos"))]
    {
        SpeechError::VozRuntime(
            "The Voz worker in the flow bar did not start. Reopen Kivo and try again.".into(),
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        SpeechError::VozUnavailable
    }
}

/// Safe fallback used by platforms without an installed Voz adapter (including
/// the Linux development harness). It never claims availability or redirects
/// audio to another engine.
#[cfg(any(test, not(any(windows, target_os = "macos"))))]
pub struct UnavailableVozRuntime;

#[cfg(any(test, not(any(windows, target_os = "macos"))))]
impl VozRuntime for UnavailableVozRuntime {
    fn model_status(&self) -> SpeechFuture<'_, Result<VozModelStatus, SpeechError>> {
        Box::pin(async {
            Ok(VozModelStatus {
                supported: false,
                downloaded: false,
                phase: VozModelPhase::Unsupported,
                progress: None,
                runtime: "Unavailable on this platform".into(),
                error: None,
            })
        })
    }

    fn download(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async { Err(SpeechError::VozUnavailable) })
    }

    fn remove_model(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async { Err(SpeechError::VozUnavailable) })
    }

    fn prepare(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async { Err(SpeechError::VozUnavailable) })
    }

    fn transcribe(
        &self,
        _samples: Vec<f32>,
        _locale: String,
    ) -> SpeechFuture<'_, Result<VozTranscript, SpeechError>> {
        Box::pin(async { Err(SpeechError::VozUnavailable) })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        path::{Path, PathBuf},
    };

    use tauri::{AppHandle, Emitter};

    use super::{
        SpeechError, SpeechFuture, VozModelPhase, VozModelStatus, VozRuntime, VozTranscript,
    };

    unsafe extern "C" {
        fn kivo_voz_status(root: *const c_char) -> bool;
        fn kivo_voz_download(
            root: *const c_char,
            callback: Option<unsafe extern "C" fn(f64, *mut c_void)>,
            context: *mut c_void,
        ) -> *mut c_char;
        fn kivo_voz_prepare(root: *const c_char) -> *mut c_char;
        fn kivo_voz_transcribe(
            samples: *const f32,
            count: isize,
            root: *const c_char,
            language: *const c_char,
        ) -> *mut c_char;
        fn kivo_voz_remove(root: *const c_char) -> bool;
        fn kivo_voz_free(pointer: *mut c_char);
    }

    fn root_cstring(root: &Path) -> Result<CString, SpeechError> {
        CString::new(root.to_string_lossy().as_bytes()).map_err(|_| SpeechError::Backend)
    }

    unsafe fn response(pointer: *mut c_char) -> Result<String, SpeechError> {
        if pointer.is_null() {
            return Err(SpeechError::VozUnavailable);
        }
        let text = unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned();
        unsafe { kivo_voz_free(pointer) };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(error) = value.get("error").and_then(serde_json::Value::as_str)
        {
            return Err(SpeechError::VozRuntime(error.to_owned()));
        }
        Ok(text)
    }

    unsafe extern "C" fn report_progress(fraction: f64, context: *mut c_void) {
        if context.is_null() {
            return;
        }
        let app = unsafe { &*(context.cast::<AppHandle>()) };
        let _ = app.emit(
            "voz-model-status",
            VozModelProgressPayload {
                phase: "downloading",
                progress: Some(fraction.clamp(0.0, 1.0)),
                error: None,
            },
        );
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct VozModelProgressPayload {
        phase: &'static str,
        progress: Option<f64>,
        error: Option<String>,
    }

    pub struct MacVozRuntime {
        app: AppHandle,
        root: PathBuf,
    }

    impl MacVozRuntime {
        pub fn new(app: AppHandle, root: PathBuf) -> Self {
            Self { app, root }
        }
    }

    impl VozRuntime for MacVozRuntime {
        fn model_status(&self) -> SpeechFuture<'_, Result<VozModelStatus, SpeechError>> {
            let root = self.root.clone();
            Box::pin(async move {
                let root = root_cstring(&root)?;
                let downloaded = unsafe { kivo_voz_status(root.as_ptr()) };
                Ok(VozModelStatus {
                    supported: true,
                    downloaded,
                    phase: if downloaded {
                        VozModelPhase::Ready
                    } else {
                        VozModelPhase::NotDownloaded
                    },
                    progress: None,
                    runtime: "Core ML · Apple Neural Engine".into(),
                    error: None,
                })
            })
        }

        fn download(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
            let root = self.root.clone();
            let app = self.app.clone();
            Box::pin(async move {
                let _ = app.emit(
                    "voz-model-status",
                    VozModelProgressPayload {
                        phase: "downloading",
                        progress: None,
                        error: None,
                    },
                );
                tokio::task::spawn_blocking(move || {
                    let root = root_cstring(&root)?;
                    let context = (&app as *const AppHandle) as usize;
                    let _downloaded = unsafe {
                        response(kivo_voz_download(
                            root.as_ptr(),
                            Some(report_progress),
                            context as *mut c_void,
                        ))?
                    };
                    let _ = app.emit(
                        "voz-model-status",
                        VozModelProgressPayload {
                            phase: "preparing",
                            progress: None,
                            error: None,
                        },
                    );
                    unsafe { response(kivo_voz_prepare(root.as_ptr())) }.map(|_| ())
                })
                .await
                .map_err(|_| SpeechError::Backend)??;
                Ok(())
            })
        }

        fn remove_model(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
            let root = self.root.clone();
            Box::pin(async move {
                let root = root_cstring(&root)?;
                if unsafe { kivo_voz_remove(root.as_ptr()) } {
                    Ok(())
                } else {
                    Err(SpeechError::VozRuntime(
                        "Could not remove the cached Voz model.".into(),
                    ))
                }
            })
        }

        fn prepare(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
            let root = self.root.clone();
            let app = self.app.clone();
            Box::pin(async move {
                let _ = app.emit(
                    "voz-model-status",
                    VozModelProgressPayload {
                        phase: "preparing",
                        progress: None,
                        error: None,
                    },
                );
                tokio::task::spawn_blocking(move || {
                    let root = root_cstring(&root)?;
                    unsafe { response(kivo_voz_prepare(root.as_ptr())) }.map(|_| ())
                })
                .await
                .map_err(|_| SpeechError::Backend)?
            })
        }

        fn transcribe(
            &self,
            samples: Vec<f32>,
            locale: String,
        ) -> SpeechFuture<'_, Result<VozTranscript, SpeechError>> {
            let root = self.root.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    let root = root_cstring(&root)?;
                    let locale = CString::new(locale).map_err(|_| SpeechError::Backend)?;
                    let raw = unsafe {
                        response(kivo_voz_transcribe(
                            samples.as_ptr(),
                            samples.len() as isize,
                            root.as_ptr(),
                            locale.as_ptr(),
                        ))?
                    };
                    let value: serde_json::Value =
                        serde_json::from_str(&raw).map_err(|_| SpeechError::Backend)?;
                    serde_json::from_value::<VozTranscript>(value).map_err(|_| SpeechError::Backend)
                })
                .await
                .map_err(|_| SpeechError::Backend)?
            })
        }
    }

    pub use MacVozRuntime as Runtime;
}

#[cfg(target_os = "macos")]
pub use macos::Runtime as MacVozRuntime;

/// Windows' tagged Voz release is the official ONNX Runtime Web build. The
/// isolated worker lives in Kivo's persistent flow-bar WebView2; Rust sends
/// captured PCM only for the duration of local inference.
#[cfg(windows)]
pub struct WindowsVozRuntime {
    app: AppHandle,
    next_id: AtomicU64,
    worker_ready: WorkerReadiness,
    pending:
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<VozWorkerReply, SpeechError>>>>,
}

#[cfg(windows)]
impl WindowsVozRuntime {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            next_id: AtomicU64::new(1),
            worker_ready: WorkerReadiness::new(),
            pending: Mutex::new(HashMap::new()),
        }
    }

    async fn request(
        &self,
        operation: &str,
        samples: Option<Vec<f32>>,
        language: Option<String>,
    ) -> Result<VozWorkerReply, SpeechError> {
        self.worker_ready
            .wait(std::time::Duration::from_secs(10))
            .await?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| SpeechError::Backend)?
            .insert(id.clone(), sender);
        let request = VozWorkerRequest {
            request_id: id.clone(),
            operation: operation.into(),
            samples,
            language,
        };
        if self
            .app
            .emit_to("flow-bar", "voz-worker-request", request)
            .is_err()
        {
            self.pending
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&id);
            return Err(SpeechError::VozUnavailable);
        }
        let timeout = match operation {
            "status" => std::time::Duration::from_secs(15),
            "remove" => std::time::Duration::from_secs(30),
            "download" => std::time::Duration::from_secs(30 * 60),
            "prepare" | "transcribe" => std::time::Duration::from_secs(5 * 60),
            _ => std::time::Duration::from_secs(60),
        };
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => Err(SpeechError::VozUnavailable),
            Err(_) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                Err(SpeechError::VozRuntime(format!(
                    "Voz {operation} timed out. Check the model and WebView2, then retry."
                )))
            }
        }
    }

    fn status_from(reply: VozWorkerReply) -> Result<VozModelStatus, SpeechError> {
        if let Some(error) = reply.error {
            return Err(SpeechError::VozRuntime(error));
        }
        Ok(VozModelStatus {
            supported: true,
            downloaded: reply.downloaded.unwrap_or(false),
            phase: reply.phase.unwrap_or(VozModelPhase::NotDownloaded),
            progress: reply.progress,
            runtime: "WebView2 · ONNX Runtime Web".into(),
            error: None,
        })
    }
}

#[cfg(windows)]
impl VozRuntime for WindowsVozRuntime {
    fn set_worker_ready(&self, ready: bool) -> Result<(), SpeechError> {
        self.worker_ready.set(ready);
        Ok(())
    }

    fn model_status(&self) -> SpeechFuture<'_, Result<VozModelStatus, SpeechError>> {
        Box::pin(async { Self::status_from(self.request("status", None, None).await?) })
    }
    fn download(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async {
            Self::status_from(self.request("download", None, None).await?).map(|_| ())
        })
    }
    fn remove_model(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async { Self::status_from(self.request("remove", None, None).await?).map(|_| ()) })
    }
    fn prepare(&self) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async {
            Self::status_from(self.request("prepare", None, None).await?).map(|_| ())
        })
    }
    fn transcribe(
        &self,
        samples: Vec<f32>,
        locale: String,
    ) -> SpeechFuture<'_, Result<VozTranscript, SpeechError>> {
        Box::pin(async move {
            let reply = self
                .request("transcribe", Some(samples), Some(locale))
                .await?;
            if let Some(error) = reply.error {
                return Err(SpeechError::VozRuntime(error));
            }
            reply.transcript.ok_or(SpeechError::VozUnavailable)
        })
    }
    fn complete_worker_request(&self, reply: VozWorkerReply) -> Result<(), SpeechError> {
        if !reply.complete {
            if let Some(phase) = reply.phase {
                let status = VozModelStatus {
                    supported: true,
                    downloaded: reply.downloaded.unwrap_or(false),
                    phase,
                    progress: reply.progress,
                    runtime: "WebView2 · ONNX Runtime Web".into(),
                    error: None,
                };
                self.app
                    .emit("voz-model-status", status)
                    .map_err(|_| SpeechError::Backend)?;
            }
            return Ok(());
        }
        let sender = self
            .pending
            .lock()
            .map_err(|_| SpeechError::Backend)?
            .remove(&reply.request_id);
        if let Some(sender) = sender {
            let _ = sender.send(Ok(reply));
        }
        Ok(())
    }
}

struct ActiveSession {
    capture: AudioCapture,
    locale: String,
}

pub struct VozSpeechEngine {
    runtime: Arc<dyn VozRuntime>,
    next_session: AtomicU64,
    sessions: Mutex<HashMap<SpeechSessionId, ActiveSession>>,
}

impl VozSpeechEngine {
    pub fn new(runtime: Arc<dyn VozRuntime>) -> Self {
        Self {
            runtime,
            next_session: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl SpeechEngine for VozSpeechEngine {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        Box::pin(async { super::local::list_microphones() })
    }

    fn start(
        &self,
        options: SpeechStartOptions,
        events: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        Box::pin(async move {
            if !matches!(&options.backend, SpeechBackend::Voz) {
                return Err(SpeechError::Backend);
            }
            let locale = options
                .locale
                .filter(|locale| supports_language(locale))
                .ok_or(SpeechError::UnsupportedVozLanguage)?;
            let status = self.runtime.model_status().await?;
            if !status.supported {
                return Err(SpeechError::VozUnavailable);
            }
            if !status.downloaded {
                return Err(SpeechError::VozUnavailable);
            }
            self.runtime.prepare().await?;
            let capture = AudioCapture::start(options.microphone_id, events).await?;
            let id = SpeechSessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
            self.sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .insert(id, ActiveSession { capture, locale });
            Ok(id)
        })
    }

    fn stop(
        &self,
        session: SpeechSessionId,
    ) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>> {
        Box::pin(async move {
            let session = self
                .sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&session)
                .ok_or(SpeechError::NotRunning)?;
            let samples = session.capture.finish().await?;
            let result = self
                .runtime
                .transcribe(samples, session.locale.clone())
                .await?;
            validate_detected_language(
                &session.locale,
                result.detected_language.as_deref(),
                result.language_reliable,
            )?;
            SpeechTranscript::from_voz(result)
        })
    }

    fn cancel(&self, session: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async move {
            let session = self
                .sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&session)
                .ok_or(SpeechError::NotRunning)?;
            session.capture.cancel().await;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::WorkerReadiness;
    use super::{
        SUPPORTED_LANGUAGES, UnavailableVozRuntime, VozModelPhase, VozSpeechEngine,
        supports_language, validate_detected_language,
    };
    use crate::speech::{
        SpeechBackend, SpeechEngine, SpeechError, SpeechEvent, SpeechStartOptions,
        SpeechTranscript, VozTranscript, VozWord,
    };

    #[test]
    fn language_gate_accepts_supported_locale_variants() {
        assert!(supports_language("en"));
        assert!(supports_language("en-GB"));
        assert!(supports_language("pt_BR"));
        assert!(supports_language("auto"));
        assert!(supports_language("EN_us"));
        assert!(!supports_language("ja-JP"));
        assert_eq!(SUPPORTED_LANGUAGES.len(), 25);
    }

    #[test]
    fn language_gate_requires_a_reliable_supported_match() {
        assert!(validate_detected_language("auto", Some("en"), true).is_ok());
        assert!(validate_detected_language("en-GB", Some("en"), true).is_ok());
        assert_eq!(
            validate_detected_language("fr", Some("en"), true),
            Err(SpeechError::VozLanguageMismatch {
                selected: "fr".into(),
                detected: "en".into()
            })
        );
        assert_eq!(
            validate_detected_language("auto", Some("ja"), true),
            Err(SpeechError::VozDetectedUnsupported("ja".into()))
        );
        assert_eq!(
            validate_detected_language("auto", Some("en"), false),
            Err(SpeechError::VozLanguageUncertain)
        );
        assert_eq!(
            validate_detected_language("auto", None, false),
            Err(SpeechError::VozLanguageUncertain)
        );
    }

    #[tokio::test]
    async fn unavailable_runtime_does_not_fall_back_to_another_engine() {
        let engine = VozSpeechEngine::new(Arc::new(UnavailableVozRuntime));
        let error = engine
            .start(
                SpeechStartOptions {
                    microphone_id: None,
                    locale: Some("en".into()),
                    backend: SpeechBackend::Voz,
                },
                Arc::new(|_: SpeechEvent| {}),
            )
            .await
            .unwrap_err();
        assert_eq!(error, SpeechError::VozUnavailable);
    }

    #[tokio::test]
    async fn unsupported_language_is_rejected_before_runtime_or_capture() {
        let engine = VozSpeechEngine::new(Arc::new(UnavailableVozRuntime));
        let error = engine
            .start(
                SpeechStartOptions {
                    microphone_id: None,
                    locale: Some("ja-JP".into()),
                    backend: SpeechBackend::Voz,
                },
                Arc::new(|_: SpeechEvent| {}),
            )
            .await
            .unwrap_err();
        assert_eq!(error, SpeechError::UnsupportedVozLanguage);
    }

    #[test]
    fn transcript_adapter_retains_word_alignment_until_text_delivery() {
        let result = VozTranscript {
            text: "hello there".into(),
            words: vec![VozWord {
                text: "hello".into(),
                start: 0.08,
                end: 0.4,
            }],
            duration_seconds: 0.9,
            processing_seconds: 0.2,
            detected_language: Some("en".into()),
            language_reliable: true,
            language_confidence: 0.98,
        };
        let transcript = SpeechTranscript::from_voz(result).unwrap();
        let (text, alignment) = transcript.into_parts();
        assert_eq!(text, "hello there");
        assert_eq!(alignment.unwrap().words[0].start, 0.08);
    }

    #[test]
    fn model_phase_uses_the_frontend_camel_case_contract() {
        assert_eq!(
            serde_json::to_string(&VozModelPhase::NotDownloaded).unwrap(),
            "\"notDownloaded\""
        );
        assert_eq!(
            serde_json::to_string(&VozModelPhase::Preparing).unwrap(),
            "\"preparing\""
        );
    }

    #[tokio::test]
    async fn worker_requests_wait_until_the_webview_worker_is_registered() {
        let readiness = WorkerReadiness::new();
        let waiting = readiness.clone();
        let request =
            tokio::spawn(async move { waiting.wait(std::time::Duration::from_secs(1)).await });
        readiness.set(true);
        assert!(request.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn missing_worker_returns_a_retryable_runtime_error() {
        let readiness = WorkerReadiness::new();
        let error = readiness
            .wait(std::time::Duration::from_millis(1))
            .await
            .unwrap_err();
        #[cfg(any(windows, target_os = "macos"))]
        assert!(matches!(error, SpeechError::VozRuntime(_)));
        #[cfg(not(any(windows, target_os = "macos")))]
        assert_eq!(error, SpeechError::VozUnavailable);
    }
}
