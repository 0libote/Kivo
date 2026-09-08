use std::{fmt, future::Future, pin::Pin};

use serde::Serialize;

use crate::ai::WritingAction;

pub type TextFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveApplication {
    pub identifier: String,
    pub display_name: String,
}

// Reserved for the isolated application-specific and clipboard strategies; the
// current adapters deliberately use Accessibility/UI Automation first.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextAccessStrategy {
    Accessibility,
    ApplicationAdapter,
    ClipboardFallback,
}

/// Captured text stays in the trusted process and is never serializable.
///
/// The platform adapter owns the native selection/range associated with `ticket`.
/// This lets replacement target the original field after the popup takes focus.
pub struct CapturedSelection {
    ticket: u64,
    text: String,
    pub application: ActiveApplication,
    pub anchor: Option<ScreenRect>,
    pub strategy: TextAccessStrategy,
}

impl CapturedSelection {
    pub fn new(
        ticket: u64,
        text: String,
        application: ActiveApplication,
        anchor: Option<ScreenRect>,
        strategy: TextAccessStrategy,
    ) -> Result<Self, TextError> {
        if text.trim().is_empty() {
            return Err(TextError::NoSelection);
        }
        Ok(Self {
            ticket,
            text,
            application,
            anchor,
            strategy,
        })
    }

    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn strategy(&self) -> TextAccessStrategy {
        self.strategy
    }
}

pub trait TextService: Send + Sync {
    fn capture_selection(&self) -> TextFuture<'_, Result<CapturedSelection, TextError>>;
    fn replace_selected_text<'a>(
        &'a self,
        selection: &'a CapturedSelection,
        replacement: &'a str,
    ) -> TextFuture<'a, Result<(), TextError>>;
    fn insert_text_at_cursor<'a>(&'a self, text: &'a str) -> TextFuture<'a, Result<(), TextError>>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TextError {
    NoSelection,
    AccessibilityPermissionRequired,
    UnsupportedApplication,
    SelectionExpired,
    ReplacementFailed,
    InsertionFailed,
    Backend,
}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoSelection => "Select some text first.",
            Self::AccessibilityPermissionRequired => "Writing Tools needs Accessibility access.",
            Self::UnsupportedApplication => "This application does not expose editable text.",
            Self::SelectionExpired => "The original selection is no longer available.",
            Self::ReplacementFailed => "The selected text couldn't be replaced.",
            Self::InsertionFailed => "The dictated text couldn't be inserted.",
            Self::Backend => "Text integration stopped unexpectedly.",
        })
    }
}

impl std::error::Error for TextError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "action")]
pub enum WritingPopupPhase {
    Hidden,
    Ready,
    Processing(WritingAction),
    Result(WritingAction),
    Error,
}

#[derive(Debug)]
pub struct WritingPopupMachine {
    phase: WritingPopupPhase,
}

impl Default for WritingPopupMachine {
    fn default() -> Self {
        Self {
            phase: WritingPopupPhase::Hidden,
        }
    }
}

impl WritingPopupMachine {
    pub fn phase(&self) -> WritingPopupPhase {
        self.phase
    }

    pub fn show(&mut self) {
        self.phase = WritingPopupPhase::Ready;
    }

    pub fn begin(&mut self, action: WritingAction) -> Result<(), WritingTransitionError> {
        if !matches!(
            self.phase,
            WritingPopupPhase::Ready | WritingPopupPhase::Result(_) | WritingPopupPhase::Error
        ) {
            return Err(self.invalid("start an action"));
        }
        self.phase = WritingPopupPhase::Processing(action);
        Ok(())
    }

    pub fn complete_replacement(&mut self) -> Result<(), WritingTransitionError> {
        let WritingPopupPhase::Processing(action) = self.phase else {
            return Err(self.invalid("complete a replacement"));
        };
        if !action.replaces_selection() {
            return Err(self.invalid("complete a replacement"));
        }
        self.phase = WritingPopupPhase::Hidden;
        Ok(())
    }

    pub fn show_result(&mut self) -> Result<(), WritingTransitionError> {
        let WritingPopupPhase::Processing(action) = self.phase else {
            return Err(self.invalid("show a result"));
        };
        if action.replaces_selection() {
            return Err(self.invalid("show a result"));
        }
        self.phase = WritingPopupPhase::Result(action);
        Ok(())
    }

    pub fn fail(&mut self) {
        self.phase = WritingPopupPhase::Error;
    }

    pub fn dismiss(&mut self) {
        self.phase = WritingPopupPhase::Hidden;
    }

    fn invalid(&self, action: &'static str) -> WritingTransitionError {
        WritingTransitionError {
            phase: self.phase,
            action,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WritingTransitionError {
    pub phase: WritingPopupPhase,
    pub action: &'static str,
}

impl fmt::Display for WritingTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot {} while writing tools are {:?}",
            self.action, self.phase
        )
    }
}

impl std::error::Error for WritingTransitionError {}

#[cfg(test)]
mod tests {
    use super::{WritingPopupMachine, WritingPopupPhase};
    use crate::ai::WritingAction;

    #[test]
    fn replacement_actions_close_after_completion() {
        let mut machine = WritingPopupMachine::default();
        machine.show();
        machine.begin(WritingAction::Proofread).unwrap();
        assert_eq!(
            machine.phase(),
            WritingPopupPhase::Processing(WritingAction::Proofread)
        );
        machine.complete_replacement().unwrap();
        assert_eq!(machine.phase(), WritingPopupPhase::Hidden);
    }

    #[test]
    fn informational_actions_morph_into_results() {
        let mut machine = WritingPopupMachine::default();
        machine.show();
        machine.begin(WritingAction::Summarize).unwrap();
        machine.show_result().unwrap();
        assert_eq!(
            machine.phase(),
            WritingPopupPhase::Result(WritingAction::Summarize)
        );
    }

    #[test]
    fn replacement_and_result_completion_are_not_interchangeable() {
        let mut machine = WritingPopupMachine::default();
        machine.show();
        machine.begin(WritingAction::KeyPoints).unwrap();
        assert!(machine.complete_replacement().is_err());
        assert_eq!(
            machine.phase(),
            WritingPopupPhase::Processing(WritingAction::KeyPoints)
        );
    }

    #[test]
    fn dismiss_is_always_immediate() {
        let mut machine = WritingPopupMachine::default();
        machine.show();
        machine.begin(WritingAction::Rewrite).unwrap();
        machine.dismiss();
        assert_eq!(machine.phase(), WritingPopupPhase::Hidden);
    }

    #[test]
    fn failed_actions_can_be_retried_with_the_original_selection() {
        let mut machine = WritingPopupMachine::default();
        machine.show();
        machine.begin(WritingAction::Proofread).unwrap();
        machine.fail();
        machine.begin(WritingAction::Proofread).unwrap();
        assert_eq!(
            machine.phase(),
            WritingPopupPhase::Processing(WritingAction::Proofread)
        );
    }
}
