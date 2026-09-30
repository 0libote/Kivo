//! Shared on-device microphone capture for batch recognizers. Capture, mono
//! downmixing, resampling, level events, and silence trimming stay in Rust.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::Resampler;

use super::{SpeechError, SpeechEvent, SpeechEventSink};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioCapture {
    pub async fn start(
        microphone_id: Option<String>,
        events: SpeechEventSink,
    ) -> Result<Self, SpeechError> {
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));
        let (started, startup) = tokio::sync::oneshot::channel();
        let thread_samples = Arc::clone(&samples);
        let thread_stop = Arc::clone(&stop);
        let thread_level = Arc::clone(&level);
        let requested_mic = microphone_id.filter(|microphone| microphone != "default");
        let handle = std::thread::Builder::new()
            .name("kivo-speech-capture".into())
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

        let pump_stop = Arc::clone(&stop);
        tokio::task::spawn_blocking(move || {
            while !pump_stop.load(Ordering::SeqCst) {
                let value = f32::from_bits(level.swap(0, Ordering::Relaxed));
                if value > 0.0 {
                    events(SpeechEvent::AudioLevel(value));
                }
                std::thread::sleep(LEVEL_INTERVAL);
            }
        });

        Ok(Self {
            stop,
            samples,
            sample_rate,
            handle: Some(handle),
        })
    }

    pub async fn finish(mut self) -> Result<Vec<f32>, SpeechError> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = tokio::task::spawn_blocking(move || handle.join()).await;
        }
        let samples = self.samples.lock().map_err(|_| SpeechError::Backend)?;
        let pcm = resample_to_16k(&samples, self.sample_rate);
        let Some((start, end)) = voiced_span(&pcm) else {
            return Err(SpeechError::NoSpeechDetected);
        };
        Ok(pcm[start..end].to_vec())
    }

    pub async fn cancel(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = tokio::task::spawn_blocking(move || handle.join()).await;
        }
        if let Ok(mut samples) = self.samples.lock() {
            samples.clear();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
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
        Some(name) => host
            .input_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find(|device| {
                    device
                        .description()
                        .map(|description| description.name() == name)
                        .unwrap_or(false)
                })
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

    macro_rules! callback {
        ($sample:ty) => {
            move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                let mut sum_squares = 0f32;
                let mut count = 0usize;
                if let Ok(mut buffer) = samples.lock() {
                    for frame in data.chunks(channels) {
                        let mono = frame
                            .iter()
                            .map(|sample| cpal::Sample::to_sample::<f32>(*sample))
                            .sum::<f32>()
                            / channels as f32;
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
                stream_config.clone(),
                callback!($sample),
                move |_error: cpal::Error| {},
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
    let _ = started.send(Ok(stream_config.sample_rate));
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
}

const VAD_FRAME_SAMPLES: usize = 256;
const VAD_THRESHOLD: f32 = 0.5;

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
    let Ok(mut resampler) = rubato::Fft::<f32>::new(
        input_rate as usize,
        TARGET_SAMPLE_RATE as usize,
        1024,
        1,
        rubato::FixedSync::Input,
    ) else {
        return samples.to_vec();
    };
    let input = match rubato::audioadapter_buffers::direct::InterleavedSlice::new(
        samples,
        1,
        samples.len(),
    ) {
        Ok(input) => input,
        Err(_) => return samples.to_vec(),
    };
    match resampler.process_all(&input, samples.len(), None) {
        Ok(processed) => processed.take_data(),
        Err(_) => samples.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::{TARGET_SAMPLE_RATE, merge_span, resample_to_16k, voiced_span};

    #[test]
    fn sample_rate_conversion_is_stable() {
        let input = vec![0.25f32; 1000];
        assert_eq!(resample_to_16k(&input, TARGET_SAMPLE_RATE), input);
        let output = resample_to_16k(&vec![0.0f32; 48_000], 48_000);
        assert!((output.len() as i64 - 16_000).abs() < 2_000);
    }

    #[test]
    fn speech_span_merges_and_clamps() {
        assert_eq!(merge_span(std::iter::empty(), 1000), None);
        assert_eq!(
            merge_span([(100, 200), (150, 300)].into_iter(), 1000),
            Some((100, 300))
        );
        assert_eq!(
            merge_span([(900, 5000)].into_iter(), 1000),
            Some((900, 1000))
        );
    }

    #[test]
    fn silence_has_no_speech_span() {
        assert!(voiced_span(&[]).is_none());
        assert!(voiced_span(&vec![0.0f32; 16_000]).is_none());
    }
}
