use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{atomic::AtomicBool, mpsc},
    thread,
};

use super::*;
use crate::{
    speech::{SpeechFuture, SpeechSessionId, SpeechTranscript},
    text::{TextAccessStrategy, TextFuture},
};

#[derive(Default)]
struct MockText {
    captures: AtomicU64,
    replacements: Mutex<Vec<(u64, String)>>,
    selected_text: Option<String>,
}

impl TextService for MockText {
    fn capture_selection(&self) -> TextFuture<'_, Result<CapturedSelection, TextError>> {
        let ticket = self.captures.fetch_add(1, Ordering::Relaxed) + 1;
        Box::pin(async move {
            CapturedSelection::new(
                ticket,
                self.selected_text.clone().ok_or(TextError::NoSelection)?,
                ActiveApplication {
                    identifier: "test-editor".into(),
                    display_name: "Test editor".into(),
                },
                None,
                TextAccessStrategy::Accessibility,
            )
        })
    }

    fn replace_selected_text<'a>(
        &'a self,
        selection: &'a CapturedSelection,
        replacement: &'a str,
    ) -> TextFuture<'a, Result<(), TextError>> {
        Box::pin(async move {
            self.replacements
                .lock()
                .unwrap()
                .push((selection.ticket(), replacement.into()));
            Ok(())
        })
    }

    fn capture_insertion_target(&self) -> Result<Box<dyn crate::text::InsertionTarget>, TextError> {
        Err(TextError::UnsupportedApplication)
    }
}

struct UnusedSpeech;

impl SpeechEngine for UnusedSpeech {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        panic!("writing summaries must not query microphones")
    }

    fn start(
        &self,
        _: SpeechStartOptions,
        _: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        panic!("writing summaries must not start dictation")
    }

    fn stop(&self, _: SpeechSessionId) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>> {
        panic!("writing summaries must not stop dictation")
    }

    fn cancel(&self, _: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        panic!("writing summaries must not cancel dictation")
    }
}

struct TestCredentials;

impl CredentialStore for TestCredentials {
    fn save_api_key(&self, _: &SecretString) -> Result<(), CredentialError> {
        panic!("writing summaries must not change credentials")
    }

    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError> {
        Ok(Some(SecretString::new("test-key".into()).unwrap()))
    }

    fn clear_api_key(&self) -> Result<(), CredentialError> {
        panic!("writing summaries must not clear credentials")
    }
}

struct UnusedSettingsRuntime;

impl SettingsRuntime for UnusedSettingsRuntime {
    fn apply(&self, _: &AppSettings, _: &AppSettings) -> Result<(), SettingsRuntimeError> {
        panic!("writing summaries must not reconfigure system settings")
    }
}

fn core(endpoint: &str, selected_text: Option<&str>) -> (Arc<AppCore>, Arc<MockText>) {
    let text = Arc::new(MockText {
        selected_text: selected_text.map(str::to_owned),
        ..MockText::default()
    });
    // Construct directly so tests never read or write the user's preferences.
    let core = AppCore {
        settings: RwLock::new(AppSettings::default()),
        settings_repository: SettingsRepository::new("unused-test-settings.json"),
        settings_runtime: Arc::new(UnusedSettingsRuntime),
        credentials: Arc::new(TestCredentials),
        ai: GeminiClient::with_endpoint(endpoint.into()).unwrap(),
        speech: Arc::new(UnusedSpeech),
        text: text.clone(),
        dictation: Mutex::new(DictationMachine::default()),
        dictation_operation: tokio::sync::Mutex::new(()),
        dictation_generation: AtomicU64::new(0),
        dictation_cancel: tokio::sync::watch::channel(()).0,
        dictation_target: Mutex::new(None),
        last_transcript: Mutex::new(None),
        writing: Mutex::new(WritingPopupMachine::default()),
        pending_selection: Mutex::new(None),
        last_result: Mutex::new(None),
        last_cursor: Mutex::new(None),
        writing_generation: AtomicU64::new(0),
        writing_cancel: tokio::sync::watch::channel(()).0,
    };
    (Arc::new(core), text)
}

const TEXT_RESPONSE: &str = r#"{"status":"completed","outputs":[{"type":"text","text":"Summary"}],"steps":[{"type":"model_output","content":[{"type":"text","text":"Summary"}]}]}"#;
const WEBSITE_RESPONSE: &str = r#"{"status":"completed","steps":[{"type":"url_context_result","result":[{"status":"success","url":"https://example.com/article"}]},{"type":"model_output","content":[{"type":"text","text":"Summary"}]}]}"#;

/// Accept one fixture connection and return the stream plus its parsed JSON
/// body, or `None` when the fixture is stopping. Shared by the single-shot
/// and multi-response fixtures so the socket framing lives in one place.
fn read_fixture_request(
    listener: &TcpListener,
    stop: &AtomicBool,
) -> Option<(TcpStream, serde_json::Value)> {
    let mut stream = loop {
        if stop.load(Ordering::Acquire) {
            return None;
        }
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) => panic!("local HTTP fixture failed: {error}"),
        }
    };
    // Accepted sockets can inherit nonblocking mode on macOS.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let (body_start, body_length) = loop {
        let mut buffer = [0; 4096];
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "HTTP request ended before its headers");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (end + 4, length);
        }
    };
    while bytes.len() < body_start + body_length {
        let mut buffer = [0; 4096];
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "HTTP request ended before its body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    let body = serde_json::from_slice(&bytes[body_start..body_start + body_length]).unwrap();
    Some((stream, body))
}

/// A single local request, with a gate for proving cancellation before an HTTP response.
struct HttpFixture {
    endpoint: String,
    arrived: Option<tokio::sync::oneshot::Receiver<serde_json::Value>>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

/// Serves a fixed sequence of responses on successive connections, reporting
/// every request body. Used to prove the backup-model retry: the first
/// request fails (e.g. 429) and the retry carries the backup model.
struct HttpSequenceFixture {
    endpoint: String,
    bodies: mpsc::Receiver<serde_json::Value>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl HttpSequenceFixture {
    fn new(responses: Vec<(u16, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/interactions", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (bodies_tx, bodies) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            for (status, response) in responses {
                let Some((mut stream, body)) = read_fixture_request(&listener, &worker_stop) else {
                    return;
                };
                let _ = bodies_tx.send(body);
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            endpoint,
            bodies,
            stop,
            worker: Some(worker),
        }
    }

    fn next_request(&mut self) -> serde_json::Value {
        self.bodies
            .recv_timeout(Duration::from_secs(3))
            .expect("expected another HTTP request")
    }
}

impl Drop for HttpSequenceFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let joined = self.worker.take().unwrap().join();
        if !thread::panicking() {
            joined.unwrap();
        }
    }
}

impl HttpFixture {
    fn new(status: u16, response: &'static str, delayed: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/interactions", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (arrived_tx, arrived) = tokio::sync::oneshot::channel();
        let (release, release_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            let Some((mut stream, body)) = read_fixture_request(&listener, &worker_stop) else {
                return;
            };
            let _ = arrived_tx.send(body);
            if delayed {
                let _ = release_rx.recv_timeout(Duration::from_secs(3));
            }
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            );
            // A cancelled client intentionally closes the connection before this write.
            let _ = stream.write_all(response.as_bytes());
        });
        Self {
            endpoint,
            arrived: Some(arrived),
            release,
            stop,
            worker: Some(worker),
        }
    }

    async fn request(&mut self) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(3), self.arrived.take().unwrap())
            .await
            .unwrap()
            .unwrap()
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.release.send(());
        let joined = self.worker.take().unwrap().join();
        if !thread::panicking() {
            joined.unwrap();
        }
    }
}

#[tokio::test]
async fn link_summaries_cannot_replace_an_existing_selection() {
    for (url, response) in [
        ("https://example.com/article", WEBSITE_RESPONSE),
        ("https://www.youtube.com/watch?v=dQw4w9WgXcQ", TEXT_RESPONSE),
    ] {
        let mut server = HttpFixture::new(200, response, false);
        let (core, text) = core(&server.endpoint, Some("Original selection"));
        core.open_writing_tools().await.unwrap();
        let result = core
            .run_writing_action(
                WritingAction::Summarize,
                None,
                Some(url.into()),
                WritingSourceKind::Link,
            )
            .await
            .unwrap();
        assert!(matches!(
            result,
            WritingOutcome::Result {
                source: Some(_),
                can_replace: false,
                ..
            }
        ));
        assert!(matches!(
            core.replace_with_last_result().await,
            Err(AppCoreError::NoResult)
        ));
        assert_eq!(
            core.writing_phase().unwrap(),
            WritingPopupPhase::Result(WritingAction::Summarize)
        );
        assert!(text.replacements.lock().unwrap().is_empty());
        assert_eq!(server.request().await["store"], false);
    }
}

#[tokio::test]
async fn selected_text_summary_preserves_explicit_replacement() {
    let server = HttpFixture::new(200, TEXT_RESPONSE, false);
    let (core, text) = core(&server.endpoint, Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    let result = core
        .run_writing_action(
            WritingAction::Summarize,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap();
    assert!(matches!(
        result,
        WritingOutcome::Result {
            source: None,
            can_replace: true,
            ..
        }
    ));
    assert!(text.replacements.lock().unwrap().is_empty());
    assert_eq!(
        core.replace_with_last_result().await.unwrap(),
        WritingOutcome::Replaced
    );
    assert_eq!(
        *text.replacements.lock().unwrap(),
        vec![(1, "Summary".into())]
    );
    assert_eq!(core.writing_phase().unwrap(), WritingPopupPhase::Hidden);
    assert!(matches!(
        core.replace_with_last_result().await,
        Err(AppCoreError::NoResult)
    ));
}

#[tokio::test]
async fn rate_limited_primary_retries_once_on_the_backup_model() {
    let mut server = HttpSequenceFixture::new(vec![
        (429, r#"{"error":{"code":"rate_limited"}}"#),
        (200, TEXT_RESPONSE),
    ]);
    let (core, text) = core(&server.endpoint, Some("Original selection"));
    core.settings.write().unwrap().ai.backup_model = Some("gemini-2.5-flash".into());
    core.open_writing_tools().await.unwrap();
    let result = core
        .run_writing_action(
            WritingAction::Proofread,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap();
    assert!(matches!(result, WritingOutcome::Replaced));
    assert_eq!(
        *text.replacements.lock().unwrap(),
        vec![(1, "Summary".into())]
    );
    let first = server.next_request();
    let second = server.next_request();
    assert_eq!(first["model"], "gemini-3.8-flash");
    assert_eq!(second["model"], "gemini-2.5-flash");
}

#[tokio::test]
async fn transient_server_error_retries_once_on_the_same_model() {
    let mut server = HttpSequenceFixture::new(vec![
        (
            500,
            r#"{"error":{"message":"Internal error encountered.","code":"api_error"}}"#,
        ),
        (200, TEXT_RESPONSE),
    ]);
    let (core, text) = core(&server.endpoint, Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    let result = core
        .run_writing_action(
            WritingAction::Proofread,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap();
    assert!(matches!(result, WritingOutcome::Replaced));
    assert_eq!(
        *text.replacements.lock().unwrap(),
        vec![(1, "Summary".into())]
    );
    let first = server.next_request();
    let second = server.next_request();
    assert_eq!(first["model"], "gemini-3.8-flash");
    assert_eq!(second["model"], "gemini-3.8-flash");
}

#[tokio::test]
async fn persistent_server_errors_fail_retryable_after_retries() {
    let internal = r#"{"error":{"message":"Internal error encountered.","code":"api_error"}}"#;
    // Exactly 4 fixtures: 1 initial + 3 retries. A 5th attempt would find no
    // server and fail differently, so this proves the retry count.
    let mut server = HttpSequenceFixture::new(vec![
        (500, internal),
        (500, internal),
        (500, internal),
        (500, internal),
    ]);
    let (core, _) = core(&server.endpoint, Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    let error = core
        .run_writing_action(
            WritingAction::Proofread,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap_err();
    for _ in 0..4 {
        assert_eq!(server.next_request()["model"], "gemini-3.8-flash");
    }
    let command = CommandError::from(error);
    assert_eq!(
        command.message,
        "Gemini hit a temporary error. Try again shortly."
    );
    assert!(command.recoverable);
}

#[test]
fn model_unavailable_error_names_the_model_and_keeps_key_guidance() {
    let error = AppCoreError::AiModelUnavailable {
        model: "gemini-3.8-flash".into(),
    };
    let command = CommandError::from(error);
    // Distinct from invalid_api_key so Test connection can advise picking
    // another model instead of re-entering the key.
    assert_eq!(command.code, "model_unavailable");
    assert!(command.message.contains("gemini-3.8-flash"));
    assert!(command.message.contains("API key works"));
}

#[test]
fn test_connection_flags_models_missing_from_list_models() {
    let models = vec![
        crate::ai::ListedAiModel {
            id: "gemini-3.8-flash".into(),
            label: "Gemini 3.8 Flash".into(),
            description: String::new(),
        },
        crate::ai::ListedAiModel {
            id: "gemini-2.5-flash".into(),
            label: "Gemini 2.5 Flash".into(),
            description: String::new(),
        },
    ];
    // Both selected models available: no error.
    assert_eq!(
        find_unavailable_model(&models, "gemini-3.8-flash", Some("gemini-2.5-flash")),
        None
    );
    // Retired primary: reported even with a healthy backup.
    assert_eq!(
        find_unavailable_model(&models, "gemini-1.5-flash", Some("gemini-2.5-flash")),
        Some("gemini-1.5-flash".into())
    );
    // Retired backup: reported by id.
    assert_eq!(
        find_unavailable_model(&models, "gemini-3.8-flash", Some("gemini-1.5-flash")),
        Some("gemini-1.5-flash".into())
    );
    // ListModels-style prefixed ids canonicalize before comparison.
    assert_eq!(
        find_unavailable_model(&models, "models/gemini-3.8-flash", None),
        None
    );
}

#[test]
fn body_only_quota_errors_are_recoverable_rate_limits() {
    let error = AppCoreError::Gemini(GeminiError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        code: Some("rate_limited".into()),
        detail: None,
    });
    let command = CommandError::from(error);
    assert_eq!(command.code, "rate_limited");
    assert!(command.recoverable);
}

#[tokio::test]
async fn non_rate_limit_errors_never_spend_backup_quota() {
    let mut server =
        HttpSequenceFixture::new(vec![(401, r#"{"error":{"code":"authentication"}}"#)]);
    let (core, _) = core(&server.endpoint, Some("Original selection"));
    core.settings.write().unwrap().ai.backup_model = Some("gemini-2.5-flash".into());
    core.open_writing_tools().await.unwrap();
    let error = core
        .run_writing_action(
            WritingAction::Proofread,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AppCoreError::Gemini(_)));
    // Exactly one request: authentication failures must not retry.
    let _ = server.next_request();
    assert!(
        server
            .bodies
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

#[tokio::test]
async fn empty_manual_text_does_not_fall_back_to_selection_and_can_be_corrected() {
    for selection in [None, Some("Original selection")] {
        let mut server = HttpFixture::new(200, TEXT_RESPONSE, false);
        let (core, text) = core(&server.endpoint, selection);
        core.open_writing_tools().await.unwrap();
        let error = core
            .run_writing_action(
                WritingAction::Summarize,
                None,
                Some("  \n".into()),
                WritingSourceKind::Text,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AppCoreError::Prompt(PromptError::EmptySource)
        ));
        assert_eq!(core.writing_phase().unwrap(), WritingPopupPhase::Error);
        let result = core
            .run_writing_action(
                WritingAction::Summarize,
                None,
                Some("Pasted transcript".into()),
                WritingSourceKind::Text,
            )
            .await
            .unwrap();
        assert!(
            matches!(result, WritingOutcome::Result { source: None, can_replace, .. } if can_replace == selection.is_some())
        );
        let request = server.request().await;
        assert!(
            request["input"]
                .as_str()
                .unwrap()
                .contains("Pasted transcript")
        );
        assert!(
            !request["input"]
                .as_str()
                .unwrap()
                .contains("Original selection")
        );
        assert!(text.replacements.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn disabled_actions_and_link_rewrites_are_rejected_without_changing_popup() {
    let (core, text) = core("http://127.0.0.1:9", Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    core.settings
        .write()
        .unwrap()
        .writing_tools
        .enabled_actions
        .retain(|action| *action != WritingAction::Summarize);
    for (action, kind) in [
        (WritingAction::Summarize, WritingSourceKind::Text),
        (WritingAction::Summarize, WritingSourceKind::Link),
        (WritingAction::Rewrite, WritingSourceKind::Link),
    ] {
        assert!(matches!(
            core.run_writing_action(action, None, Some("https://example.com".into()), kind)
                .await,
            Err(AppCoreError::ActionDisabled)
        ));
        assert_eq!(core.writing_phase().unwrap(), WritingPopupPhase::Ready);
    }
    assert!(text.replacements.lock().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_link_fails_without_rewriting_and_allows_text_fallback() {
    let server = HttpFixture::new(200, TEXT_RESPONSE, false);
    let (core, text) = core(&server.endpoint, Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    let error = core
        .run_writing_action(
            WritingAction::Summarize,
            None,
            Some("file:///private/article.txt".into()),
            WritingSourceKind::Link,
        )
        .await
        .unwrap_err();
    assert_eq!(core.writing_phase().unwrap(), WritingPopupPhase::Error);
    assert!(!matches!(error, AppCoreError::WritingTransition(_)));
    let result = core
        .run_writing_action(
            WritingAction::Summarize,
            None,
            Some("Pasted article".into()),
            WritingSourceKind::Text,
        )
        .await
        .unwrap();
    assert!(matches!(
        result,
        WritingOutcome::Result { source: None, .. }
    ));
    assert!(text.replacements.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dismiss_cancels_network_work_immediately_and_reopen_ignores_late_responses() {
    for (status, response, kind, source) in [
        (
            200,
            WEBSITE_RESPONSE,
            WritingSourceKind::Link,
            "https://example.com/article",
        ),
        (
            503,
            r#"{"error":{"code":503}}"#,
            WritingSourceKind::Text,
            "Selected article",
        ),
    ] {
        let mut server = HttpFixture::new(status, response, true);
        let (core, text) = core(&server.endpoint, Some("Original selection"));
        core.open_writing_tools().await.unwrap();
        let request_core = core.clone();
        let request = tokio::spawn(async move {
            request_core
                .run_writing_action(WritingAction::Summarize, None, Some(source.into()), kind)
                .await
        });
        server.request().await;
        assert_eq!(
            core.dismiss_writing_tools().unwrap(),
            WritingPopupPhase::Hidden
        );
        core.open_writing_tools().await.unwrap();
        let result = tokio::time::timeout(Duration::from_millis(500), request)
            .await
            .expect("cancellation must finish without waiting for the HTTP response")
            .unwrap();
        assert!(matches!(result, Err(AppCoreError::WritingCancelled)));
        drop(server);
        assert_eq!(core.writing_phase().unwrap(), WritingPopupPhase::Ready);
        assert_eq!(
            core.writing_context().unwrap().initial_text,
            "Original selection"
        );
        assert_eq!(text.captures.load(Ordering::Relaxed), 2);
        assert!(text.replacements.lock().unwrap().is_empty());
        assert!(core.last_result.lock().unwrap().is_none());
    }
}

#[test]
fn permission_requirements_match_each_desktop() {
    // Host-parameterized: runs identically on macOS and Windows CI, so a
    // requirement flipped for one desktop fails on both.
    use crate::{config::HostPlatform, platform::PermissionKind};

    // macOS: input monitoring (Fn-hold) and OS speech recognition gate the app.
    assert!(permission_required_for(
        PermissionKind::Accessibility,
        HostPlatform::Macos
    ));
    assert!(permission_required_for(
        PermissionKind::InputMonitoring,
        HostPlatform::Macos
    ));
    assert!(permission_required_for(
        PermissionKind::Microphone,
        HostPlatform::Macos
    ));
    assert!(permission_required_for(
        PermissionKind::SpeechRecognition,
        HostPlatform::Macos
    ));
    // Windows: desktop SAPI + microphone need no OS permission prompt;
    // only accessibility (text replacement) and microphone stay required.
    assert!(permission_required_for(
        PermissionKind::Accessibility,
        HostPlatform::Windows
    ));
    assert!(!permission_required_for(
        PermissionKind::InputMonitoring,
        HostPlatform::Windows
    ));
    assert!(permission_required_for(
        PermissionKind::Microphone,
        HostPlatform::Windows
    ));
    assert!(!permission_required_for(
        PermissionKind::SpeechRecognition,
        HostPlatform::Windows
    ));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_lists_installed_speech_languages() {
    // Only installed desktop engines are listed, plus the system default.
    if let Some(languages) = windows_speech_languages() {
        assert!(!languages.is_empty());
        assert_eq!(languages[0].code, "auto");
        assert!(languages.iter().all(|language| language.installed));
    }
}

struct DictationText {
    inserted: Arc<AtomicU64>,
    valid: Arc<AtomicBool>,
}
struct TestInsertion {
    inserted: Arc<AtomicU64>,
    valid: Arc<AtomicBool>,
}
impl crate::text::InsertionTarget for TestInsertion {
    fn insert(&self, _: &str) -> Result<(), TextError> {
        if !self.valid.load(Ordering::Acquire) {
            return Err(TextError::SelectionExpired);
        }
        self.inserted.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl TextService for DictationText {
    fn capture_selection(&self) -> TextFuture<'_, Result<CapturedSelection, TextError>> {
        Box::pin(async { Err(TextError::NoSelection) })
    }
    fn replace_selected_text<'a>(
        &'a self,
        _: &'a CapturedSelection,
        _: &'a str,
    ) -> TextFuture<'a, Result<(), TextError>> {
        Box::pin(async { Err(TextError::SelectionExpired) })
    }
    fn capture_insertion_target(&self) -> Result<Box<dyn crate::text::InsertionTarget>, TextError> {
        Ok(Box::new(TestInsertion {
            inserted: self.inserted.clone(),
            valid: self.valid.clone(),
        }))
    }
}

struct GatedSpeech {
    start_entered: tokio::sync::Notify,
    start_release: tokio::sync::Notify,
    stop_entered: tokio::sync::Notify,
    stop_release: tokio::sync::Notify,
    delay_start: bool,
    delay_stop: bool,
}
impl SpeechEngine for GatedSpeech {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn start(
        &self,
        _: SpeechStartOptions,
        _: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        Box::pin(async {
            self.start_entered.notify_one();
            if self.delay_start {
                self.start_release.notified().await;
            }
            Ok(SpeechSessionId(1))
        })
    }
    fn stop(&self, _: SpeechSessionId) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>> {
        Box::pin(async {
            self.stop_entered.notify_one();
            if self.delay_stop {
                self.stop_release.notified().await;
            }
            SpeechTranscript::new("Keep these words".into())
        })
    }
    fn cancel(&self, _: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async { Ok(()) })
    }
}

fn dictation_core(
    endpoint: &str,
    delay_start: bool,
    delay_stop: bool,
) -> (Arc<AppCore>, Arc<GatedSpeech>, Arc<DictationText>) {
    let (mut core, _) = core(endpoint, None);
    let speech = Arc::new(GatedSpeech {
        start_entered: Default::default(),
        start_release: Default::default(),
        stop_entered: Default::default(),
        stop_release: Default::default(),
        delay_start,
        delay_stop,
    });
    let text = Arc::new(DictationText {
        inserted: Arc::new(AtomicU64::new(0)),
        valid: Arc::new(AtomicBool::new(true)),
    });
    let state = Arc::get_mut(&mut core).unwrap();
    state.speech = speech.clone();
    state.text = text.clone();
    state.settings.write().unwrap().dictation.improve_with_ai = false;
    (core, speech, text)
}

#[tokio::test]
async fn dictation_cancel_during_start_and_stop_never_inserts() {
    for during_start in [true, false] {
        let (core, speech, text) =
            dictation_core("http://127.0.0.1:1", during_start, !during_start);
        let task_core = core.clone();
        let task = tokio::spawn(async move {
            let phase = task_core.begin_dictation(Arc::new(|_| {})).await?;
            if phase == DictationPhase::Hidden {
                return Ok(phase);
            }
            task_core.finish_dictation().await
        });
        if during_start {
            speech.start_entered.notified().await;
        } else {
            speech.stop_entered.notified().await;
        }
        core.cancel_dictation().await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(250), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            DictationPhase::Hidden
        );
        speech.start_release.notify_one();
        speech.stop_release.notify_one();
        assert_eq!(text.inserted.load(Ordering::Relaxed), 0);
        assert!(core.recovery_text().unwrap().is_none());
    }
}

#[tokio::test]
async fn dictation_cancel_during_ai_discards_late_result() {
    let mut server = HttpFixture::new(200, TEXT_RESPONSE, true);
    let (core, _, text) = dictation_core(&server.endpoint, false, false);
    core.settings.write().unwrap().dictation.improve_with_ai = true;
    core.begin_dictation(Arc::new(|_| {})).await.unwrap();
    let task_core = core.clone();
    let task = tokio::spawn(async move { task_core.finish_dictation().await });
    server.request().await;
    core.cancel_dictation().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(250), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        DictationPhase::Hidden
    );
    drop(server);
    assert_eq!(text.inserted.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn changed_dictation_target_preserves_recovery_and_next_session_works() {
    let (core, _, text) = dictation_core("http://127.0.0.1:1", false, false);
    core.begin_dictation(Arc::new(|_| {})).await.unwrap();
    text.valid.store(false, Ordering::Release);
    assert!(matches!(
        core.finish_dictation().await,
        Err(AppCoreError::Text(TextError::SelectionExpired))
    ));
    assert_eq!(text.inserted.load(Ordering::Relaxed), 0);
    assert_eq!(
        core.recovery_text().unwrap().as_deref(),
        Some("Keep these words")
    );
    core.clear_recovery().unwrap();
    text.valid.store(true, Ordering::Release);
    core.begin_dictation(Arc::new(|_| {})).await.unwrap();
    assert_eq!(
        core.finish_dictation().await.unwrap(),
        DictationPhase::Success
    );
    assert_eq!(text.inserted.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn failed_replacement_returns_copyable_result() {
    let server = HttpFixture::new(200, TEXT_RESPONSE, false);
    let (mut core, _) = core(&server.endpoint, Some("Original selection"));
    core.open_writing_tools().await.unwrap();
    Arc::get_mut(&mut core).unwrap().text = Arc::new(DictationText {
        inserted: Arc::new(AtomicU64::new(0)),
        valid: Arc::new(AtomicBool::new(false)),
    });
    let result = core
        .run_writing_action(
            WritingAction::Proofread,
            None,
            None,
            WritingSourceKind::Text,
        )
        .await
        .unwrap();
    assert!(matches!(
        result,
        WritingOutcome::Result {
            can_replace: false,
            ..
        }
    ));
    assert!(core.last_result.lock().unwrap().is_some());
}

struct RecordingSettingsRuntime(Mutex<Vec<AppSettings>>);
impl SettingsRuntime for RecordingSettingsRuntime {
    fn apply(&self, _: &AppSettings, next: &AppSettings) -> Result<(), SettingsRuntimeError> {
        self.0.lock().unwrap().push(next.clone());
        Ok(())
    }
}

struct FailingSettingsRuntime;
impl SettingsRuntime for FailingSettingsRuntime {
    fn apply(&self, _: &AppSettings, _: &AppSettings) -> Result<(), SettingsRuntimeError> {
        Err(SettingsRuntimeError::ShortcutUnavailable)
    }
}

#[test]
fn failed_settings_apply_keeps_previous_preferences() {
    // The OS-side apply path is fallible (shortcut conflicts, autostart
    // denial): a failure must abort the save with memory and file untouched.
    let (mut core, _) = core("http://127.0.0.1:1", None);
    let state = Arc::get_mut(&mut core).unwrap();
    state.settings_runtime = Arc::new(FailingSettingsRuntime);
    let previous = core.settings().unwrap();
    let mut changed = previous.clone();
    changed.general.launch_at_login = !previous.general.launch_at_login;
    assert!(core.save_settings(changed).is_err());
    assert_eq!(core.settings().unwrap(), previous);
}

#[test]
fn failed_settings_write_restores_runtime_and_keeps_previous_preferences() {
    let (mut core, _) = core("http://127.0.0.1:1", None);
    let runtime = Arc::new(RecordingSettingsRuntime(Mutex::new(Vec::new())));
    // A regular file cannot be a parent directory, on either desktop OS.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent = std::env::temp_dir().join(format!("kivo-settings-failure-{unique}"));
    std::fs::write(&parent, b"sentinel").unwrap();
    let state = Arc::get_mut(&mut core).unwrap();
    state.settings_repository = SettingsRepository::new(parent.join("settings.json"));
    state.settings_runtime = runtime.clone();
    let previous = core.settings().unwrap();
    let mut changed = previous.clone();
    changed.general.launch_at_login = !previous.general.launch_at_login;
    assert!(core.save_settings(changed.clone()).is_err());
    assert_eq!(core.settings().unwrap(), previous);
    assert_eq!(*runtime.0.lock().unwrap(), vec![changed, previous]);
    assert_eq!(std::fs::read(&parent).unwrap(), b"sentinel");
    std::fs::remove_file(parent).unwrap();
}
