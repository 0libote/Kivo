use std::{fmt, future::Future, pin::Pin, sync::Arc};

use serde::{Deserialize, Serialize};

pub mod capture;
pub mod local;
pub mod model_store;
pub mod router;
pub mod voz;

pub type SpeechFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type SpeechEventSink = Arc<dyn Fn(SpeechEvent) + Send + Sync>;

/// Which recognizer a dictation should use. `System` is the operating-system
/// engine (SAPI / SpeechAnalyzer / the Linux test bench); `Local` runs a
/// downloaded GGML model on-device; `Voz` runs Desert Ant's batch recognizer.
/// The engine behind the trait is chosen per session so switching never
/// restarts the app.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum SpeechBackend {
    #[default]
    System,
    Local {
        model_id: String,
    },
    Voz,
}

/// A Voz result keeps its word alignment alongside the text. It is deliberately
/// not serializable: only the final text crosses the existing trusted bridge.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VozWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VozTranscript {
    pub text: String,
    pub words: Vec<VozWord>,
    pub duration_seconds: f64,
    pub processing_seconds: f64,
    pub detected_language: Option<String>,
    pub language_reliable: bool,
    pub language_confidence: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechStartOptions {
    pub microphone_id: Option<String>,
    pub locale: Option<String>,
    pub backend: SpeechBackend,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SpeechSessionId(pub u64);

pub enum SpeechEvent {
    AudioLevel(f32),
    SpeechDetected,
}

/// Transcript text is intentionally neither `Debug` nor `Serialize`.
pub struct SpeechTranscript {
    text: String,
    /// Retained through Kivo's internal delivery path for future alignment
    /// features; never serialized into frontend state.
    pub(crate) voz: Option<VozTranscript>,
}

impl SpeechTranscript {
    pub fn new(text: String) -> Result<Self, SpeechError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(SpeechError::NoSpeechDetected);
        }
        Ok(Self {
            text: text.to_owned(),
            voz: None,
        })
    }

    pub fn from_voz(result: VozTranscript) -> Result<Self, SpeechError> {
        let text = result.text.trim();
        if text.is_empty() {
            return Err(SpeechError::NoSpeechDetected);
        }
        Ok(Self {
            text: text.to_owned(),
            voz: Some(result),
        })
    }

    pub fn into_parts(self) -> (String, Option<VozTranscript>) {
        (self.text, self.voz)
    }
}

pub trait SpeechEngine: Send + Sync {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>>;
    fn start(
        &self,
        options: SpeechStartOptions,
        events: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>>;
    fn stop(
        &self,
        session: SpeechSessionId,
    ) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>>;
    fn cancel(&self, session: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SpeechError {
    MicrophonePermissionRequired,
    SpeechPermissionRequired,
    MicrophoneUnavailable,
    RecognitionUnavailable,
    AlreadyRunning,
    NotRunning,
    NoSpeechDetected,
    LocalModelUnavailable,
    VozUnavailable,
    #[cfg(any(windows, target_os = "macos"))]
    VozRuntime(String),
    VozLanguageUncertain,
    VozDetectedUnsupported(String),
    VozLanguageMismatch {
        selected: String,
        detected: String,
    },
    UnsupportedVozLanguage,
    Backend,
}

impl fmt::Display for SpeechError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(any(windows, target_os = "macos"))]
        if let Self::VozRuntime(message) = self {
            return formatter.write_str(message);
        }
        match self {
            Self::VozLanguageUncertain => return formatter.write_str("Kivo could not reliably identify the spoken language. Choose a language in Settings → Dictation or select another engine."),
            Self::VozDetectedUnsupported(language) => return write!(formatter, "Voz does not support the detected language ({language}). Choose System or Kivo On-device."),
            Self::VozLanguageMismatch { selected, detected } => return write!(formatter, "The audio sounds like {detected}, but Kivo is set to {selected}. Change the dictation language or choose another engine."),
            _ => {}
        }
        formatter.write_str(match self {
            Self::MicrophonePermissionRequired => "Microphone access is required for dictation.",
            Self::SpeechPermissionRequired => {
                "Speech Recognition access is required for dictation."
            }
            Self::MicrophoneUnavailable => "The selected microphone is unavailable.",
            Self::RecognitionUnavailable => "Speech recognition is unavailable. Check that the selected speech language is installed.",
            Self::AlreadyRunning => "Dictation is already active.",
            Self::NotRunning => "Dictation is not active.",
            Self::NoSpeechDetected => "No speech was detected.",
            Self::LocalModelUnavailable => {
                "Download a local speech model in Settings → Dictation, then try again."
            }
            Self::VozUnavailable => "Voz is unavailable on this platform. Choose System or another on-device engine.",
            #[cfg(any(windows, target_os = "macos"))]
            Self::VozRuntime(_) => "Voz could not complete the local speech operation. Retry or choose another engine.",
            Self::UnsupportedVozLanguage => "Voz does not support the selected language. Choose one of its supported languages in Settings → Dictation.",
            Self::VozLanguageUncertain => "Kivo could not reliably identify the spoken language.",
            Self::VozDetectedUnsupported(_) => "Voz does not support the detected language.",
            Self::VozLanguageMismatch { .. } => "The detected language does not match the selected language.",
            Self::Backend => "Dictation stopped. Check your default microphone and microphone access.",
        })
    }
}

impl std::error::Error for SpeechError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationPhase {
    Hidden,
    Starting,
    Listening,
    Processing,
    Success,
    Error,
}

#[derive(Debug)]
pub struct DictationMachine {
    phase: DictationPhase,
    session: Option<SpeechSessionId>,
}

impl Default for DictationMachine {
    fn default() -> Self {
        Self {
            phase: DictationPhase::Hidden,
            session: None,
        }
    }
}

impl DictationMachine {
    pub fn prepare(&mut self) -> Result<(), DictationTransitionError> {
        if !matches!(
            self.phase,
            DictationPhase::Hidden | DictationPhase::Success | DictationPhase::Error
        ) {
            return Err(self.invalid("prepare"));
        }
        self.phase = DictationPhase::Starting;
        Ok(())
    }
    pub fn phase(&self) -> DictationPhase {
        self.phase
    }

    pub fn begin(&mut self, session: SpeechSessionId) -> Result<(), DictationTransitionError> {
        if !matches!(
            self.phase,
            DictationPhase::Hidden
                | DictationPhase::Starting
                | DictationPhase::Success
                | DictationPhase::Error
        ) {
            return Err(self.invalid("begin"));
        }
        self.phase = DictationPhase::Listening;
        self.session = Some(session);
        Ok(())
    }

    pub fn release(&mut self) -> Result<SpeechSessionId, DictationTransitionError> {
        if self.phase != DictationPhase::Listening {
            return Err(self.invalid("release"));
        }
        self.phase = DictationPhase::Processing;
        self.session.ok_or_else(|| self.invalid("release"))
    }

    pub fn complete(&mut self) -> Result<(), DictationTransitionError> {
        if self.phase != DictationPhase::Processing {
            return Err(self.invalid("complete"));
        }
        self.phase = DictationPhase::Success;
        self.session = None;
        Ok(())
    }

    pub fn fail(&mut self) {
        self.phase = DictationPhase::Error;
        self.session = None;
    }

    pub fn cancel(&mut self) -> Option<SpeechSessionId> {
        let session = self.session.take();
        self.phase = DictationPhase::Hidden;
        session
    }

    fn invalid(&self, action: &'static str) -> DictationTransitionError {
        DictationTransitionError {
            phase: self.phase,
            action,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DictationTransitionError {
    pub phase: DictationPhase,
    pub action: &'static str,
}

impl fmt::Display for DictationTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot {} while dictation is {:?}",
            self.action, self.phase
        )
    }
}

impl std::error::Error for DictationTransitionError {}

#[cfg(test)]
mod tests {
    use super::{DictationMachine, DictationPhase, SpeechSessionId};

    #[test]
    fn hold_release_success_flow_is_valid() {
        let mut machine = DictationMachine::default();
        machine.begin(SpeechSessionId(7)).unwrap();
        assert_eq!(machine.phase(), DictationPhase::Listening);
        assert_eq!(machine.release().unwrap(), SpeechSessionId(7));
        assert_eq!(machine.phase(), DictationPhase::Processing);
        machine.complete().unwrap();
        assert_eq!(machine.phase(), DictationPhase::Success);
        machine.cancel();
        assert_eq!(machine.phase(), DictationPhase::Hidden);
    }

    #[test]
    fn escape_cancels_without_processing() {
        let mut machine = DictationMachine::default();
        machine.begin(SpeechSessionId(3)).unwrap();
        assert_eq!(machine.cancel(), Some(SpeechSessionId(3)));
        assert_eq!(machine.phase(), DictationPhase::Hidden);
    }

    #[test]
    fn invalid_transitions_do_not_mutate_state() {
        let mut machine = DictationMachine::default();
        assert!(machine.release().is_err());
        assert_eq!(machine.phase(), DictationPhase::Hidden);
        assert!(machine.complete().is_err());
        assert_eq!(machine.phase(), DictationPhase::Hidden);
    }
}
