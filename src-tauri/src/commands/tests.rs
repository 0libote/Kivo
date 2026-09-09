use std::{
    io::{Read, Write},
    net::TcpListener,
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

    fn insert_text_at_cursor<'a>(&'a self, _: &'a str) -> TextFuture<'a, Result<(), TextError>> {
        panic!("writing summaries must not invoke dictation insertion")
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

/// A single local request, with a gate for proving cancellation before an HTTP response.
struct HttpFixture {
    endpoint: String,
    arrived: Option<tokio::sync::oneshot::Receiver<serde_json::Value>>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
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
            let mut stream = loop {
                if worker_stop.load(Ordering::Acquire) {
                    return;
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
            let body =
                serde_json::from_slice(&bytes[body_start..body_start + body_length]).unwrap();
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
