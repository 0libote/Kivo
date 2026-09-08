use std::{fmt, future::Future, pin::Pin, sync::Arc};

use serde::Serialize;

pub type SpeechFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type SpeechEventSink = Arc<dyn Fn(SpeechEvent) + Send + Sync>;

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
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SpeechSessionId(pub u64);

pub enum SpeechEvent {
    AudioLevel(f32),
    SpeechDetected,
}

/// Transcript text is intentionally neither `Debug` nor `Serialize`.
pub struct SpeechTranscript(String);

impl SpeechTranscript {
    pub fn new(text: String) -> Result<Self, SpeechError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(SpeechError::NoSpeechDetected);
        }
        Ok(Self(text.to_owned()))
    }

    pub fn into_text(self) -> String {
        self.0
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
    Backend,
}

impl fmt::Display for SpeechError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MicrophonePermissionRequired => "Microphone access is required for dictation.",
            Self::SpeechPermissionRequired => {
                "Speech Recognition access is required for dictation."
            }
            Self::MicrophoneUnavailable => "The selected microphone is unavailable.",
            Self::RecognitionUnavailable => "Speech recognition is currently unavailable.",
            Self::AlreadyRunning => "Dictation is already active.",
            Self::NotRunning => "Dictation is not active.",
            Self::NoSpeechDetected => "No speech was detected.",
            Self::Backend => "Dictation stopped unexpectedly.",
        })
    }
}

impl std::error::Error for SpeechError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationPhase {
    Hidden,
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
    pub fn phase(&self) -> DictationPhase {
        self.phase
    }

    pub fn begin(&mut self, session: SpeechSessionId) -> Result<(), DictationTransitionError> {
        if !matches!(
            self.phase,
            DictationPhase::Hidden | DictationPhase::Success | DictationPhase::Error
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
