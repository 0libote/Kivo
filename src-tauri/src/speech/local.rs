//! On-device dictation: records the default input device, resamples to the
//! 16 kHz mono f32 the models expect, and transcribes with `transcribe-cpp`.
//!
//! The engine is deliberately batch-oriented: Kivo's dictation is hold-to-talk,
//! so the whole utterance is captured and transcribed when the user releases.
//! That keeps the streaming complexity out of the hot path (matching how the
//! OS engines already behave) while every model still benefits from GPU/CPU
//! acceleration inside transcribe.cpp.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use cpal::traits::{DeviceTrait, HostTrait};
use transcribe_cpp::{Model, ModelOptions, RunOptions, Session};

use super::{
    MicrophoneDevice, SpeechBackend, SpeechEngine, SpeechError, SpeechEventSink, SpeechFuture,
    SpeechSessionId, SpeechStartOptions, SpeechTranscript, capture::AudioCapture,
    model_store::ModelStore,
};

struct LoadedModel {
    path: std::path::PathBuf,
    session: Session,
}

struct ActiveSession {
    capture: AudioCapture,
    locale: Option<String>,
    vocabulary: Vec<crate::config::VocabularyEntry>,
    whisper_hint: bool,
}

pub struct LocalSpeechEngine {
    store: Arc<ModelStore>,
    next_session: AtomicU64,
    sessions: Mutex<HashMap<SpeechSessionId, ActiveSession>>,
    loaded: Mutex<Option<LoadedModel>>,
}

impl LocalSpeechEngine {
    pub fn new(store: Arc<ModelStore>) -> Self {
        // Static builds do not strictly need this, but it is a harmless no-op
        // there and required before the first model load in dynamic builds.
        let _ = transcribe_cpp::init_backends_default();
        Self {
            store,
            next_session: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            loaded: Mutex::new(None),
        }
    }

    fn effective_model_id<'a>(&self, model_id: &'a str) -> &'a str {
        if model_id.trim().is_empty() {
            self.store
                .first_downloaded()
                .unwrap_or(super::model_store::DEFAULT_LOCAL_MODEL_ID)
        } else {
            model_id
        }
    }

    fn resolve_model(&self, model_id: &str) -> Result<std::path::PathBuf, SpeechError> {
        let id = self.effective_model_id(model_id);
        self.store
            .path_for(id)
            .ok_or(SpeechError::LocalModelUnavailable)
    }

    /// Custom-vocabulary `initial_prompt` for Whisper-family models. Words
    /// only: meanings live in the AI prompts, where there is room for them.
    /// Other families reject the Whisper run extension, so they transcribe
    /// unchanged and rely on the AI prompt protection instead. Capped to the
    /// same prompt terms as the AI hint so a huge import stays bounded.
    fn whisper_initial_prompt(vocabulary: &[crate::config::VocabularyEntry]) -> Option<String> {
        let terms: Vec<&str> = vocabulary
            .iter()
            .map(|entry| entry.word.trim())
            .filter(|word| !word.is_empty())
            .take(crate::ai::MAX_VOCABULARY_PROMPT_TERMS)
            .collect();
        if terms.is_empty() {
            return None;
        }
        Some(format!("Vocabulary: {}.", terms.join(", ")))
    }

    async fn ensure_loaded(&self, path: &Path) -> Result<(), SpeechError> {
        let already_loaded = self
            .loaded
            .lock()
            .map_err(|_| SpeechError::Backend)?
            .as_ref()
            .is_some_and(|loaded| loaded.path == path);
        if already_loaded {
            return Ok(());
        }
        let path = path.to_owned();
        let load_path = path.clone();
        let model = tokio::task::spawn_blocking(move || {
            Model::load_with(&load_path, &ModelOptions::default())
        })
        .await
        .map_err(|_| SpeechError::Backend)?
        .map_err(|_| SpeechError::RecognitionUnavailable)?;
        let session = model
            .session()
            .map_err(|_| SpeechError::RecognitionUnavailable)?;
        *self.loaded.lock().map_err(|_| SpeechError::Backend)? =
            Some(LoadedModel { path, session });
        Ok(())
    }

    /// Runs the loaded model over a finished utterance, returning the session
    /// to the cache so the next dictation does not pay model load again.
    async fn transcribe(
        &self,
        pcm: Vec<f32>,
        locale: Option<String>,
        vocabulary: Vec<crate::config::VocabularyEntry>,
        whisper_hint: bool,
    ) -> Result<SpeechTranscript, SpeechError> {
        let mut loaded = self
            .loaded
            .lock()
            .map_err(|_| SpeechError::Backend)?
            .take()
            .ok_or(SpeechError::RecognitionUnavailable)?;
        let language = locale.map(|tag| {
            tag.split(['-', '_'])
                .next()
                .unwrap_or(tag.as_str())
                .to_owned()
        });
        let family = if whisper_hint {
            Self::whisper_initial_prompt(&vocabulary).map(|initial_prompt| {
                transcribe_cpp::RunExtension::Whisper(transcribe_cpp::WhisperRunOptions {
                    initial_prompt: Some(initial_prompt),
                    ..Default::default()
                })
            })
        } else {
            None
        };
        let (result, loaded) = tokio::task::spawn_blocking(move || {
            let options = RunOptions {
                language,
                family,
                ..RunOptions::default()
            };
            let result = loaded.session.run(&pcm, &options);
            (result, loaded)
        })
        .await
        .map_err(|_| SpeechError::Backend)?;
        *self.loaded.lock().map_err(|_| SpeechError::Backend)? = Some(loaded);
        let transcript = result.map_err(|_| SpeechError::RecognitionUnavailable)?;
        SpeechTranscript::new(transcript.text)
    }
}

impl SpeechEngine for LocalSpeechEngine {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        Box::pin(async { list_microphones() })
    }

    fn start(
        &self,
        options: SpeechStartOptions,
        events: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        Box::pin(async move {
            let model_id = match &options.backend {
                SpeechBackend::Local { model_id } => model_id.clone(),
                SpeechBackend::System => String::new(),
            };
            let path = self.resolve_model(&model_id)?;
            self.ensure_loaded(&path).await?;

            let capture = AudioCapture::start(options.microphone_id, events).await?;
            // The hint is decided up front from the catalog family so a
            // non-Whisper model never receives an extension it rejects.
            let whisper_hint = !options.vocabulary.is_empty()
                && ModelStore::is_whisper_family(self.effective_model_id(&model_id));

            let id = SpeechSessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
            self.sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .insert(
                    id,
                    ActiveSession {
                        capture,
                        locale: options.locale,
                        vocabulary: options.vocabulary,
                        whisper_hint,
                    },
                );
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
            let pcm = session.capture.finish().await?;
            self.transcribe(
                pcm,
                session.locale,
                session.vocabulary,
                session.whisper_hint,
            )
            .await
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

pub(super) fn list_microphones() -> Result<Vec<MicrophoneDevice>, SpeechError> {
    let host = cpal::default_host();
    // cpal 0.18 replaced Device::name() with Device::description().
    let default_name = host
        .default_input_device()
        .and_then(|device| device.description().ok())
        .map(|description| description.name().to_owned());
    let mut devices = vec![MicrophoneDevice {
        id: "default".into(),
        name: "System Default".into(),
        is_default: true,
    }];
    if let Ok(inputs) = host.input_devices() {
        for device in inputs {
            let Ok(description) = device.description() else {
                continue;
            };
            let name = description.name().to_owned();
            if default_name.as_deref() == Some(name.as_str()) {
                continue;
            }
            devices.push(MicrophoneDevice {
                id: name.clone(),
                name,
                is_default: false,
            });
        }
    }
    Ok(devices)
}
