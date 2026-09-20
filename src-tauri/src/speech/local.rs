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
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{FftFixedIn, Resampler};
use transcribe_cpp::{Model, ModelOptions, RunOptions, Session};

use super::{
    MicrophoneDevice, SpeechBackend, SpeechEngine, SpeechError, SpeechEvent, SpeechEventSink,
    SpeechFuture, SpeechSessionId, SpeechStartOptions, SpeechTranscript, model_store::ModelStore,
};

const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Level refresh cadence for the Flow Bar; the audio callback only stores a
/// value, so no IPC happens on the real-time thread.
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

struct LoadedModel {
    path: std::path::PathBuf,
    session: Session,
}

struct ActiveSession {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    locale: Option<String>,
    handle: Option<std::thread::JoinHandle<()>>,
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

    fn resolve_model(&self, model_id: &str) -> Result<std::path::PathBuf, SpeechError> {
        let id = if model_id.trim().is_empty() {
            self.store
                .first_downloaded()
                .unwrap_or(super::model_store::DEFAULT_LOCAL_MODEL_ID)
        } else {
            model_id
        };
        self.store
            .path_for(id)
            .ok_or(SpeechError::LocalModelUnavailable)
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
        let (result, loaded) = tokio::task::spawn_blocking(move || {
            let options = RunOptions {
                language,
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
        Box::pin(async {
            let host = cpal::default_host();
            let default_name = host
                .default_input_device()
                .and_then(|device| device.name().ok());
            let mut devices = vec![MicrophoneDevice {
                id: "default".into(),
                name: "System Default".into(),
                is_default: true,
            }];
            if let Ok(inputs) = host.input_devices() {
                for device in inputs {
                    let Ok(name) = device.name() else { continue };
                    if default_name.as_deref() == Some(name.as_str()) {
                        continue;
                    }
                    devices.push(MicrophoneDevice {
                        id: name.clone(),
                        name: name.clone(),
                        is_default: false,
                    });
                }
            }
            Ok(devices)
        })
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

            let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let level = Arc::new(AtomicU32::new(0));
            let (started, startup) = tokio::sync::oneshot::channel();
            let thread_samples = Arc::clone(&samples);
            let thread_stop = Arc::clone(&stop);
            let thread_level = Arc::clone(&level);
            let requested_mic = options
                .microphone_id
                .filter(|microphone| microphone != "default");
            let handle = std::thread::Builder::new()
                .name("kivo-local-capture".into())
                .spawn(move || {
                    run_capture(
                        requested_mic,
                        thread_samples,
                        thread_stop,
                        thread_level,
                        started,
                    )
                })
                .map_err(|_| SpeechError::MicrophoneUnavailable)?;

            let sample_rate = match tokio::time::timeout(Duration::from_secs(10), startup).await {
                Ok(Ok(Ok(sample_rate))) => sample_rate,
                Ok(Ok(Err(error))) => {
                    stop.store(true, Ordering::SeqCst);
                    let _ = handle.join();
                    return Err(error);
                }
                _ => {
                    stop.store(true, Ordering::SeqCst);
                    let _ = handle.join();
                    return Err(SpeechError::MicrophoneUnavailable);
                }
            };

            // Pump stored levels into core events without touching the audio thread.
            let pump_stop = Arc::clone(&stop);
            let pump_level = Arc::clone(&level);
            tokio::task::spawn_blocking(move || {
                while !pump_stop.load(Ordering::SeqCst) {
                    let value = f32::from_bits(pump_level.swap(0, Ordering::Relaxed));
                    if value > 0.0 {
                        events(SpeechEvent::AudioLevel(value));
                    }
                    std::thread::sleep(LEVEL_INTERVAL);
                }
            });

            let id = SpeechSessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
            self.sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .insert(
                    id,
                    ActiveSession {
                        stop,
                        samples,
                        sample_rate,
                        locale: options.locale,
                        handle: Some(handle),
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
            let mut session = self
                .sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&session)
                .ok_or(SpeechError::NotRunning)?;
            session.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = session.handle.take() {
                let _ = tokio::task::spawn_blocking(move || handle.join()).await;
            }
            let pcm = {
                let samples = session.samples.lock().map_err(|_| SpeechError::Backend)?;
                resample_to_16k(&samples, session.sample_rate)
            };
            // Trim to the spoken span so silence is never sent to the model
            // (Whisper hallucinates words on silence). No speech is a clean
            // "nothing detected" instead of a bogus transcript.
            let Some((start, end)) = voiced_span(&pcm) else {
                return Err(SpeechError::NoSpeechDetected);
            };
            self.transcribe(pcm[start..end].to_vec(), session.locale)
                .await
        })
    }

    fn cancel(&self, session: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async move {
            let mut session = self
                .sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .remove(&session)
                .ok_or(SpeechError::NotRunning)?;
            session.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = session.handle.take() {
                let _ = tokio::task::spawn_blocking(move || handle.join()).await;
            }
            session
                .samples
                .lock()
                .map(|mut samples| samples.clear())
                .ok();
            Ok(())
        })
    }
}

fn run_capture(
    requested_mic: Option<String>,
    samples: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    started: tokio::sync::oneshot::Sender<Result<u32, SpeechError>>,
) {
    let host = cpal::default_host();
    let device = match requested_mic {
        // cpal ids are device names; fall back to the default when the stored
        // platform id (a Windows SAPI id, say) has no matching input device.
        Some(name) => host
            .input_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find(|device| device.name().ok().as_deref() == Some(name.as_str()))
            })
            .or_else(|| host.default_input_device()),
        None => host.default_input_device(),
    };
    let Some(device) = device else {
        let _ = started.send(Err(SpeechError::MicrophoneUnavailable));
        return;
    };
    let Ok(config) = device.default_input_config() else {
        let _ = started.send(Err(SpeechError::MicrophoneUnavailable));
        return;
    };
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let channels = stream_config.channels as usize;

    // The same callback body for every sample format; only the input element
    // type differs, so the conversion is a macro instead of four copies.
    macro_rules! callback {
        ($sample:ty) => {
            move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                let mut sum_squares = 0f32;
                let mut count = 0usize;
                // ponytail: one Mutex shared only with the audio thread while
                // recording, so there is no contention to glitch on. Swap for
                // a ring buffer only if a device's callback ever blocks here.
                if let Ok(mut buffer) = samples.lock() {
                    for frame in data.chunks(channels) {
                        let mut mono = 0f32;
                        for sample in frame {
                            mono += cpal::Sample::to_sample::<f32>(*sample);
                        }
                        let mono = mono / channels as f32;
                        buffer.push(mono);
                        sum_squares += mono * mono;
                        count += 1;
                    }
                }
                if count > 0 {
                    level.store(
                        (sum_squares / count as f32).sqrt().to_bits(),
                        Ordering::Relaxed,
                    );
                }
            }
        };
    }

    macro_rules! open {
        ($sample:ty) => {
            device.build_input_stream(
                &stream_config,
                callback!($sample),
                move |_error: cpal::StreamError| {},
                None,
            )
        };
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => open!(f32),
        cpal::SampleFormat::I16 => open!(i16),
        cpal::SampleFormat::U16 => open!(u16),
        _ => {
            let _ = started.send(Err(SpeechError::MicrophoneUnavailable));
            return;
        }
    };
    let stream = match stream {
        Ok(stream) => stream,
        Err(_) => {
            let _ = started.send(Err(SpeechError::MicrophoneUnavailable));
            return;
        }
    };
    if stream.play().is_err() {
        let _ = started.send(Err(SpeechError::MicrophoneUnavailable));
        return;
    }
    let _ = started.send(Ok(stream_config.sample_rate.0));
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
}

/// earshot's required frame: 16 ms of 16 kHz audio.
const VAD_FRAME_SAMPLES: usize = 256;
/// earshot scores above 0.5 are generally voice.
const VAD_THRESHOLD: f32 = 0.5;

/// The sample range covering all detected speech, or `None` when the clip is
/// silent. Uses earshot, a pure-Rust VAD, so the same code runs on every
/// platform with no model file to ship.
fn voiced_span(samples: &[f32]) -> Option<(usize, usize)> {
    let mut detector = earshot::Detector::default();
    let mut spans = Vec::new();
    let mut index = 0;
    while index + VAD_FRAME_SAMPLES <= samples.len() {
        let score = detector.predict_f32(&samples[index..index + VAD_FRAME_SAMPLES]);
        if score >= VAD_THRESHOLD {
            spans.push((index, index + VAD_FRAME_SAMPLES));
        }
        index += VAD_FRAME_SAMPLES;
    }
    merge_span(spans.into_iter(), samples.len())
}

/// First speech sample to the last, ignoring per-segment gaps. Pure so the
/// span math is testable without a VAD.
fn merge_span(spans: impl Iterator<Item = (usize, usize)>, len: usize) -> Option<(usize, usize)> {
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    for (start, end) in spans {
        let start = start.min(len);
        let end = end.min(len);
        if start >= end {
            continue;
        }
        first.get_or_insert(start);
        last = last.max(end);
    }
    first.map(|start| (start, last))
}

fn resample_to_16k(samples: &[f32], input_rate: u32) -> Vec<f32> {
    if input_rate == TARGET_SAMPLE_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let chunk = 1024usize;
    let Ok(mut resampler) = FftFixedIn::<f32>::new(
        input_rate as usize,
        TARGET_SAMPLE_RATE as usize,
        chunk,
        1,
        1,
    ) else {
        return samples.to_vec();
    };
    let mut output = Vec::with_capacity(
        samples.len() * TARGET_SAMPLE_RATE as usize / input_rate as usize + chunk,
    );
    let mut index = 0;
    while index + chunk <= samples.len() {
        if let Ok(processed) = resampler.process(&[&samples[index..index + chunk]], None) {
            output.extend_from_slice(&processed[0]);
        }
        index += chunk;
    }
    if index < samples.len()
        && let Ok(processed) = resampler.process_partial(Some(&[&samples[index..]]), None)
    {
        output.extend_from_slice(&processed[0]);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{TARGET_SAMPLE_RATE, merge_span, resample_to_16k, voiced_span};

    #[test]
    fn passthrough_at_target_rate() {
        let input = vec![0.25f32; 1000];
        assert_eq!(resample_to_16k(&input, TARGET_SAMPLE_RATE), input);
    }

    #[test]
    fn resamples_to_expected_length() {
        // 1 second of 48 kHz audio should land near 1 second at 16 kHz.
        let input = vec![0.0f32; 48_000];
        let output = resample_to_16k(&input, 48_000);
        let expected = 16_000i64;
        assert!(
            (output.len() as i64 - expected).abs() < 2_000,
            "expected ~{expected} samples, got {}",
            output.len()
        );
    }

    #[test]
    fn merge_span_covers_first_to_last_speech() {
        assert_eq!(merge_span(std::iter::empty(), 1000), None);
        assert_eq!(merge_span([(100, 200)].into_iter(), 1000), Some((100, 200)));
        assert_eq!(
            merge_span([(100, 200), (150, 300)].into_iter(), 1000),
            Some((100, 300))
        );
        // Clamped to the buffer; empty or inverted spans are ignored.
        assert_eq!(
            merge_span([(900, 5000)].into_iter(), 1000),
            Some((900, 1000))
        );
        assert_eq!(
            merge_span([(50, 50), (10, 20)].into_iter(), 1000),
            Some((10, 20))
        );
    }

    #[test]
    fn silence_has_no_speech_span() {
        assert!(voiced_span(&[]).is_none());
        assert!(voiced_span(&vec![0.0f32; 16_000]).is_none());
    }

    #[test]
    fn preserves_signal_energy() {
        // A 1 kHz tone at 48 kHz resampled to 16 kHz should keep its amplitude.
        let input: Vec<f32> = (0..48_000)
            .map(|index| (2.0 * std::f32::consts::PI * 1000.0 * index as f32 / 48_000.0).sin())
            .collect();
        let output = resample_to_16k(&input, 48_000);
        let peak = output.iter().fold(0f32, |acc, value| acc.max(value.abs()));
        assert!(
            peak > 0.9,
            "expected a full-amplitude tone, peak was {peak}"
        );
    }
}
