use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    ai::{
        GeminiClient, GeminiError, LinkSource, PromptError, WritingAction,
        dictation_cleanup_prompt, writing_prompt,
    },
    config::{
        AppSettings, LanguagePreference, SettingsError, SettingsRepository, SettingsRuntime,
        SettingsRuntimeError,
    },
    security::{CredentialError, CredentialStatus, CredentialStore, SecretString},
    speech::{
        DictationMachine, DictationPhase, MicrophoneDevice, SpeechEngine, SpeechError,
        SpeechEventSink, SpeechStartOptions,
    },
    text::{
        ActiveApplication, CapturedSelection, ScreenPoint, ScreenRect, TextError, TextService,
        WritingPopupMachine, WritingPopupPhase, WritingTransitionError,
    },
};

const DICTATION_AI_DEADLINE: Duration = Duration::from_secs(4);

pub struct AppCore {
    settings: RwLock<AppSettings>,
    settings_repository: SettingsRepository,
    settings_runtime: Arc<dyn SettingsRuntime>,
    credentials: Arc<dyn CredentialStore>,
    ai: GeminiClient,
    speech: Arc<dyn SpeechEngine>,
    text: Arc<dyn TextService>,
    dictation: Mutex<DictationMachine>,
    writing: Mutex<WritingPopupMachine>,
    pending_selection: Mutex<Option<Arc<CapturedSelection>>>,
    last_result: Mutex<Option<StoredWritingResult>>,
    last_cursor: Mutex<Option<ScreenPoint>>,
    writing_generation: AtomicU64,
    writing_cancel: tokio::sync::watch::Sender<()>,
}

impl AppCore {
    pub fn new(
        settings_repository: SettingsRepository,
        settings_runtime: Arc<dyn SettingsRuntime>,
        credentials: Arc<dyn CredentialStore>,
        ai: GeminiClient,
        speech: Arc<dyn SpeechEngine>,
        text: Arc<dyn TextService>,
    ) -> Result<Self, AppCoreError> {
        let settings = settings_repository.load()?;
        Ok(Self {
            settings: RwLock::new(settings),
            settings_repository,
            settings_runtime,
            credentials,
            ai,
            speech,
            text,
            dictation: Mutex::new(DictationMachine::default()),
            writing: Mutex::new(WritingPopupMachine::default()),
            pending_selection: Mutex::new(None),
            last_result: Mutex::new(None),
            last_cursor: Mutex::new(None),
            writing_generation: AtomicU64::new(0),
            writing_cancel: tokio::sync::watch::channel(()).0,
        })
    }

    pub fn settings(&self) -> Result<AppSettings, AppCoreError> {
        Ok(self
            .settings
            .read()
            .map_err(|_| AppCoreError::Unavailable)?
            .clone())
    }

    pub fn save_settings(&self, settings: AppSettings) -> Result<AppSettings, AppCoreError> {
        let settings = settings.validate_and_normalize()?;
        let previous = self.settings()?;

        self.settings_runtime.apply(&previous, &settings)?;
        if let Err(error) = self.settings_repository.save(&settings) {
            let _ = self.settings_runtime.apply(&settings, &previous);
            return Err(error.into());
        }

        *self
            .settings
            .write()
            .map_err(|_| AppCoreError::Unavailable)? = settings.clone();
        Ok(settings)
    }

    pub fn credential_status(&self) -> Result<CredentialStatus, AppCoreError> {
        self.credentials.status().map_err(Into::into)
    }

    pub fn save_api_key(&self, api_key: SecretString) -> Result<CredentialStatus, AppCoreError> {
        self.credentials.save_api_key(&api_key)?;
        Ok(CredentialStatus { configured: true })
    }

    pub fn clear_api_key(&self) -> Result<CredentialStatus, AppCoreError> {
        self.credentials.clear_api_key()?;
        Ok(CredentialStatus { configured: false })
    }

    pub async fn test_api_key(&self) -> Result<CredentialStatus, AppCoreError> {
        let api_key = self
            .credentials
            .load_api_key()?
            .ok_or(AppCoreError::AiNotConfigured)?;
        self.ai.test_key(&api_key).await?;
        Ok(CredentialStatus { configured: true })
    }

    pub fn dictation_phase(&self) -> Result<DictationPhase, AppCoreError> {
        Ok(self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .phase())
    }

    pub fn writing_phase(&self) -> Result<WritingPopupPhase, AppCoreError> {
        Ok(self
            .writing
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .phase())
    }

    pub async fn microphones(&self) -> Result<Vec<MicrophoneDevice>, AppCoreError> {
        self.speech.microphones().await.map_err(Into::into)
    }

    pub async fn begin_dictation(
        &self,
        events: SpeechEventSink,
    ) -> Result<DictationPhase, AppCoreError> {
        let settings = self.settings()?.dictation;
        let locale = match settings.language {
            LanguagePreference::Auto => None,
            LanguagePreference::Locale { tag } => Some(tag),
        };
        let session = self
            .speech
            .start(
                SpeechStartOptions {
                    microphone_id: settings.microphone_id,
                    locale,
                },
                events,
            )
            .await?;
        let mut machine = self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?;
        machine.begin(session)?;
        Ok(machine.phase())
    }

    pub async fn finish_dictation(&self) -> Result<DictationPhase, AppCoreError> {
        let session = {
            let mut machine = self
                .dictation
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?;
            machine.release()?
        };

        let transcript = match self.speech.stop(session).await {
            Ok(transcript) => transcript,
            Err(error) => {
                self.fail_dictation();
                return Err(error.into());
            }
        };

        let settings = self.settings()?.dictation;
        let mut final_text = transcript.into_text();
        if settings.improve_with_ai
            && let Ok(Some(api_key)) = self.credentials.load_api_key()
            && let Ok(prompt) = dictation_cleanup_prompt(&final_text)
            && let Ok(Ok(cleaned)) =
                tokio::time::timeout(DICTATION_AI_DEADLINE, self.ai.generate(&api_key, &prompt))
                    .await
        {
            final_text = cleaned;
        }

        if let Err(error) = self.text.insert_text_at_cursor(&final_text).await {
            self.fail_dictation();
            return Err(error.into());
        }

        let mut machine = self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?;
        machine.complete()?;
        Ok(machine.phase())
    }

    pub async fn cancel_dictation(&self) -> Result<DictationPhase, AppCoreError> {
        let session = self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .cancel();
        if let Some(session) = session {
            // Cancellation is already reflected in the UI; a late native error must
            // not resurrect the flow bar or insert text.
            let _ = self.speech.cancel(session).await;
        }
        Ok(DictationPhase::Hidden)
    }

    pub async fn open_writing_tools(&self) -> Result<WritingPopupContext, AppCoreError> {
        // Invalidate the previous request before capture can yield or fail.
        let generation = {
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            let generation = self.invalidate_writing();
            machine.dismiss();
            self.clear_writing_context()?;
            generation
        };
        let cursor = self.text.cursor_position();
        let captured = self.text.capture_selection().await;
        let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
        self.check_writing_generation(generation)?;
        *self
            .last_cursor
            .lock()
            .map_err(|_| AppCoreError::Unavailable)? = cursor;
        let context = match captured {
            Ok(selection) => {
                let selection = Arc::new(selection);
                let context = WritingPopupContext {
                    application: selection.application.clone(),
                    anchor: selection.anchor,
                    cursor,
                    has_selection: true,
                    initial_text: selection.text().to_owned(),
                };
                *self
                    .pending_selection
                    .lock()
                    .map_err(|_| AppCoreError::Unavailable)? = Some(selection);
                context
            }
            Err(TextError::NoSelection | TextError::UnsupportedApplication) => {
                WritingPopupContext::chat(cursor)
            }
            Err(error) => return Err(error.into()),
        };
        machine.show();
        Ok(context)
    }

    pub fn writing_context(&self) -> Result<WritingPopupContext, AppCoreError> {
        let cursor = *self
            .last_cursor
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?;
        match self
            .pending_selection
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .clone()
        {
            Some(selection) => Ok(WritingPopupContext {
                application: selection.application.clone(),
                anchor: selection.anchor,
                cursor,
                has_selection: true,
                initial_text: selection.text().to_owned(),
            }),
            // Chat mode (or a cleared context): still a valid popup state.
            None => Ok(WritingPopupContext::chat(cursor)),
        }
    }

    pub async fn run_writing_action(
        &self,
        action: WritingAction,
        custom_instruction: Option<String>,
        source_override: Option<String>,
        source_kind: WritingSourceKind,
    ) -> Result<WritingOutcome, AppCoreError> {
        if action != WritingAction::Chat
            && !self
                .settings()?
                .writing_tools
                .enabled_actions
                .contains(&action)
        {
            return Err(AppCoreError::ActionDisabled);
        }
        if source_kind == WritingSourceKind::Link && action != WritingAction::Summarize {
            return Err(AppCoreError::ActionDisabled);
        }
        let (generation, mut cancellation, selection) = {
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            machine.begin(action)?;
            *self
                .last_result
                .lock()
                .map_err(|_| AppCoreError::Unavailable)? = None;
            (
                self.writing_generation.fetch_add(1, Ordering::AcqRel) + 1,
                self.writing_cancel.subscribe(),
                self.pending_selection
                    .lock()
                    .map_err(|_| AppCoreError::Unavailable)?
                    .clone(),
            )
        };
        let generate = async {
            let source_text = source_override
                .as_deref()
                .or_else(|| selection.as_ref().map(|value| value.text()))
                .ok_or(PromptError::EmptySource)?;
            let api_key = self
                .credentials
                .load_api_key()?
                .ok_or(AppCoreError::AiNotConfigured)?;
            if source_kind == WritingSourceKind::Link {
                let source = LinkSource::parse(source_text)?;
                let result = self.ai.summarize_link(&api_key, &source).await?;
                Ok::<_, AppCoreError>((result, Some(source)))
            } else {
                let prompt = writing_prompt(action, source_text, custom_instruction.as_deref())?;
                let result = self.ai.generate(&api_key, &prompt).await?;
                Ok((result, None))
            }
        };
        let generated = tokio::select! {
            biased;
            _ = cancellation.changed() => return Err(AppCoreError::WritingCancelled),
            result = generate => result,
        };
        let (result, source) = {
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            self.check_writing_generation(generation)?;
            match generated {
                Ok(value) => value,
                Err(error) => {
                    machine.fail();
                    return Err(error);
                }
            }
        };
        if action.replaces_selection() {
            // The captured ticket belongs to this request, never a later popup.
            let replacement = match selection.as_deref() {
                Some(selection) => self
                    .text
                    .replace_selected_text(selection, &result)
                    .await
                    .map_err(AppCoreError::from),
                None => Err(AppCoreError::SelectionExpired),
            };
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            self.check_writing_generation(generation)?;
            if let Err(error) = replacement {
                machine.fail();
                return Err(error);
            }
            machine.complete_replacement()?;
            self.clear_writing_context()?;
            Ok(WritingOutcome::Replaced)
        } else {
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            self.check_writing_generation(generation)?;
            machine.show_result()?;
            let can_replace = source.is_none() && selection.is_some();
            *self
                .last_result
                .lock()
                .map_err(|_| AppCoreError::Unavailable)? = Some(StoredWritingResult {
                text: Arc::from(result.as_str()),
                can_replace,
            });
            Ok(WritingOutcome::Result {
                markdown: result,
                source,
                can_replace,
            })
        }
    }

    pub async fn replace_with_last_result(&self) -> Result<WritingOutcome, AppCoreError> {
        let (selection, result, generation) = {
            let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
            if !matches!(machine.phase(), WritingPopupPhase::Result(_)) {
                return Err(AppCoreError::NoResult);
            }
            let stored = self
                .last_result
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?;
            let stored = stored
                .as_ref()
                .filter(|result| result.can_replace)
                .ok_or(AppCoreError::NoResult)?;
            let selection = self
                .pending_selection
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?
                .clone()
                .ok_or(AppCoreError::SelectionExpired)?;
            // Reserve the result so repeated clicks cannot paste twice.
            machine.begin(WritingAction::Rewrite)?;
            (
                selection,
                Arc::clone(&stored.text),
                self.writing_generation.load(Ordering::Acquire),
            )
        };
        let replacement = self
            .text
            .replace_selected_text(&selection, result.as_ref())
            .await;
        let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
        self.check_writing_generation(generation)?;
        if let Err(error) = replacement {
            machine.fail();
            return Err(error.into());
        }
        machine.dismiss();
        self.invalidate_writing();
        self.clear_writing_context()?;
        Ok(WritingOutcome::Replaced)
    }

    pub fn dismiss_writing_tools(&self) -> Result<WritingPopupPhase, AppCoreError> {
        let mut machine = self.writing.lock().map_err(|_| AppCoreError::Unavailable)?;
        self.invalidate_writing();
        machine.dismiss();
        self.clear_writing_context()?;
        Ok(WritingPopupPhase::Hidden)
    }

    // Call while holding `writing`, including when committing a response or error.
    fn invalidate_writing(&self) -> u64 {
        let generation = self.writing_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.writing_cancel.send_replace(());
        generation
    }

    fn check_writing_generation(&self, generation: u64) -> Result<(), AppCoreError> {
        if self.writing_generation.load(Ordering::Acquire) != generation {
            return Err(AppCoreError::WritingCancelled);
        }
        Ok(())
    }

    fn fail_dictation(&self) {
        if let Ok(mut machine) = self.dictation.lock() {
            machine.fail();
        }
    }

    fn clear_writing_context(&self) -> Result<(), AppCoreError> {
        *self
            .pending_selection
            .lock()
            .map_err(|_| AppCoreError::Unavailable)? = None;
        *self
            .last_result
            .lock()
            .map_err(|_| AppCoreError::Unavailable)? = None;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingPopupContext {
    pub application: ActiveApplication,
    pub anchor: Option<ScreenRect>,
    pub cursor: Option<ScreenPoint>,
    pub has_selection: bool,
    /// Text shown in the manual text box: the captured highlight, or empty
    /// for quick chat. Only sent to Kivo's own popup.
    pub initial_text: String,
}

impl WritingPopupContext {
    fn chat(cursor: Option<ScreenPoint>) -> Self {
        Self {
            application: ActiveApplication {
                identifier: "quick-chat".into(),
                display_name: "Quick chat".into(),
            },
            anchor: None,
            cursor,
            has_selection: false,
            initial_text: String::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WritingOutcome {
    Replaced,
    Result {
        markdown: String,
        source: Option<LinkSource>,
        can_replace: bool,
    },
}

struct StoredWritingResult {
    text: Arc<str>,
    can_replace: bool,
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WritingSourceKind {
    #[default]
    Text,
    Link,
}

#[derive(Debug)]
pub enum AppCoreError {
    Unavailable,
    AiNotConfigured,
    ActionDisabled,
    SelectionExpired,
    NoResult,
    WritingCancelled,
    Settings(SettingsError),
    SettingsRuntime(SettingsRuntimeError),
    Credential(CredentialError),
    Gemini(GeminiError),
    Prompt(PromptError),
    Speech(SpeechError),
    DictationTransition(crate::speech::DictationTransitionError),
    Text(TextError),
    WritingTransition(WritingTransitionError),
}

impl std::fmt::Display for AppCoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.user_message())
    }
}

impl std::error::Error for AppCoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Settings(error) => Some(error),
            Self::SettingsRuntime(error) => Some(error),
            Self::Credential(error) => Some(error),
            Self::Gemini(error) => Some(error),
            Self::Prompt(error) => Some(error),
            Self::Speech(error) => Some(error),
            Self::DictationTransition(error) => Some(error),
            Self::Text(error) => Some(error),
            Self::WritingTransition(error) => Some(error),
            _ => None,
        }
    }
}

impl AppCoreError {
    fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "core_unavailable",
            Self::AiNotConfigured => "ai_not_configured",
            Self::ActionDisabled => "action_disabled",
            Self::SelectionExpired => "selection_expired",
            Self::NoResult => "no_result",
            Self::WritingCancelled => "writing_cancelled",
            Self::Settings(_) => "settings",
            Self::SettingsRuntime(_) => "settings_runtime",
            Self::Credential(_) => "credential",
            Self::Gemini(error) => error.code(),
            Self::Prompt(_) => "invalid_prompt",
            Self::Speech(_) => "speech",
            Self::DictationTransition(_) => "dictation_state",
            Self::Text(_) => "text_integration",
            Self::WritingTransition(_) => "writing_state",
        }
    }

    pub(crate) fn user_message(&self) -> String {
        match self {
            Self::Unavailable => "The application is temporarily unavailable.".into(),
            Self::AiNotConfigured => "Add your Google AI Studio API key first.".into(),
            Self::ActionDisabled => "That writing action is disabled in Settings.".into(),
            Self::SelectionExpired => "The original selection is no longer available.".into(),
            Self::NoResult => "There is no result to replace the selection with.".into(),
            Self::WritingCancelled => "The writing request was cancelled.".into(),
            Self::Gemini(error) => error.user_message().into(),
            Self::Settings(error) => error.to_string(),
            Self::SettingsRuntime(error) => error.to_string(),
            Self::Credential(error) => error.to_string(),
            Self::Prompt(error) => error.to_string(),
            Self::Speech(error) => error.to_string(),
            Self::DictationTransition(_) | Self::WritingTransition(_) => {
                "That action is not available right now.".into()
            }
            Self::Text(error) => error.to_string(),
        }
    }
}

macro_rules! from_core_error {
    ($variant:ident, $source:ty) => {
        impl From<$source> for AppCoreError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_core_error!(Settings, SettingsError);
from_core_error!(SettingsRuntime, SettingsRuntimeError);
from_core_error!(Credential, CredentialError);
from_core_error!(Gemini, GeminiError);
from_core_error!(Prompt, PromptError);
from_core_error!(Speech, SpeechError);
from_core_error!(DictationTransition, crate::speech::DictationTransitionError);
from_core_error!(Text, TextError);
from_core_error!(WritingTransition, WritingTransitionError);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl From<AppCoreError> for CommandError {
    fn from(error: AppCoreError) -> Self {
        let recoverable = match &error {
            AppCoreError::Gemini(GeminiError::Transport(_))
            | AppCoreError::Gemini(GeminiError::Incomplete(_))
            | AppCoreError::Gemini(GeminiError::EmptyResponse)
            | AppCoreError::Speech(SpeechError::NoSpeechDetected)
            | AppCoreError::Speech(SpeechError::RecognitionUnavailable)
            | AppCoreError::Speech(SpeechError::Backend) => true,
            AppCoreError::Gemini(GeminiError::Api { status, .. }) => {
                *status == reqwest::StatusCode::TOO_MANY_REQUESTS
            }
            _ => false,
        };
        Self {
            code: error.code().into(),
            message: error.user_message(),
            recoverable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSettings {
    pub launch_at_login: bool,
    pub theme: String,
    pub show_idle_flow_bar: bool,
    pub start_in_background: bool,
    pub dictation_shortcut: String,
    pub microphone_id: Option<String>,
    pub improve_dictation_with_ai: bool,
    pub dictation_language: String,
    pub sound_feedback: bool,
    pub writing_shortcut: String,
    pub enabled_writing_actions: Vec<String>,
    pub writing_popup_anchor: String,
    pub writing_popup_x: f64,
    pub writing_popup_y: f64,
    pub writing_popup_width: f64,
    pub writing_popup_height: f64,
    pub writing_allow_manual_text: bool,
    pub onboarding_complete: bool,
}

impl From<AppSettings> for FrontendSettings {
    fn from(settings: AppSettings) -> Self {
        Self {
            launch_at_login: settings.general.launch_at_login,
            theme: match settings.general.theme {
                crate::config::ThemePreference::System => "system",
                crate::config::ThemePreference::Light => "light",
                crate::config::ThemePreference::Dark => "dark",
            }
            .into(),
            show_idle_flow_bar: settings.general.show_flow_bar_while_idle,
            start_in_background: settings.general.start_in_background,
            dictation_shortcut: settings.dictation.shortcut.accelerator,
            microphone_id: settings.dictation.microphone_id,
            improve_dictation_with_ai: settings.dictation.improve_with_ai,
            dictation_language: match settings.dictation.language {
                LanguagePreference::Auto => "auto".into(),
                LanguagePreference::Locale { tag } => tag,
            },
            sound_feedback: settings.dictation.sound_feedback,
            writing_shortcut: settings.writing_tools.shortcut.accelerator,
            enabled_writing_actions: settings
                .writing_tools
                .enabled_actions
                .into_iter()
                .map(action_id)
                .map(str::to_owned)
                .collect(),
            writing_popup_anchor: match settings.writing_tools.popup_anchor {
                crate::config::PopupAnchor::Cursor => "cursor",
                crate::config::PopupAnchor::Selection => "selection",
                crate::config::PopupAnchor::Fixed => "fixed",
            }
            .into(),
            writing_popup_x: settings.writing_tools.popup_fixed_x,
            writing_popup_y: settings.writing_tools.popup_fixed_y,
            writing_popup_width: settings.writing_tools.popup_width,
            writing_popup_height: settings.writing_tools.popup_height,
            writing_allow_manual_text: settings.writing_tools.allow_manual_text,
            onboarding_complete: settings.general.onboarding_complete,
        }
    }
}

impl TryFrom<FrontendSettings> for AppSettings {
    type Error = AppCoreError;

    fn try_from(settings: FrontendSettings) -> Result<Self, Self::Error> {
        let theme = match settings.theme.as_str() {
            "system" => crate::config::ThemePreference::System,
            "light" => crate::config::ThemePreference::Light,
            "dark" => crate::config::ThemePreference::Dark,
            _ => return Err(SettingsError::InvalidJson(invalid_settings_json()).into()),
        };
        let enabled_actions = settings
            .enabled_writing_actions
            .iter()
            .map(|action| parse_action(action))
            .collect::<Result<Vec<_>, _>>()?;
        // "chat" is a mode, not a toggleable action; it is always available
        // and never persisted in the enabled list.
        let enabled_actions = enabled_actions
            .into_iter()
            .filter(|action| *action != WritingAction::Chat)
            .collect();
        let popup_anchor = match settings.writing_popup_anchor.as_str() {
            "selection" => crate::config::PopupAnchor::Selection,
            "fixed" => crate::config::PopupAnchor::Fixed,
            _ => crate::config::PopupAnchor::Cursor,
        };
        Ok(AppSettings {
            schema_version: crate::config::SETTINGS_SCHEMA_VERSION,
            general: crate::config::GeneralSettings {
                launch_at_login: settings.launch_at_login,
                theme,
                show_flow_bar_while_idle: settings.show_idle_flow_bar,
                start_in_background: settings.start_in_background,
                onboarding_complete: settings.onboarding_complete,
            },
            dictation: crate::config::DictationSettings {
                shortcut: crate::config::ShortcutBinding::new(settings.dictation_shortcut),
                microphone_id: settings.microphone_id,
                improve_with_ai: settings.improve_dictation_with_ai,
                language: if settings.dictation_language == "auto" {
                    LanguagePreference::Auto
                } else {
                    LanguagePreference::Locale {
                        tag: settings.dictation_language,
                    }
                },
                sound_feedback: settings.sound_feedback,
            },
            writing_tools: crate::config::WritingToolsSettings {
                shortcut: crate::config::ShortcutBinding::new(settings.writing_shortcut),
                enabled_actions,
                popup_anchor,
                popup_fixed_x: settings.writing_popup_x,
                popup_fixed_y: settings.writing_popup_y,
                popup_width: settings.writing_popup_width,
                popup_height: settings.writing_popup_height,
                allow_manual_text: settings.writing_allow_manual_text,
            },
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    launch_at_login: Option<bool>,
    theme: Option<String>,
    show_idle_flow_bar: Option<bool>,
    start_in_background: Option<bool>,
    dictation_shortcut: Option<String>,
    microphone_id: Option<Option<String>>,
    improve_dictation_with_ai: Option<bool>,
    dictation_language: Option<String>,
    sound_feedback: Option<bool>,
    writing_shortcut: Option<String>,
    enabled_writing_actions: Option<Vec<String>>,
    writing_popup_anchor: Option<String>,
    writing_popup_x: Option<f64>,
    writing_popup_y: Option<f64>,
    writing_popup_width: Option<f64>,
    writing_popup_height: Option<f64>,
    writing_allow_manual_text: Option<bool>,
    onboarding_complete: Option<bool>,
}

impl SettingsPatch {
    fn apply(self, mut settings: FrontendSettings) -> FrontendSettings {
        macro_rules! assign {
            ($field:ident) => {
                if let Some(value) = self.$field {
                    settings.$field = value;
                }
            };
        }
        assign!(launch_at_login);
        assign!(theme);
        assign!(show_idle_flow_bar);
        assign!(start_in_background);
        assign!(dictation_shortcut);
        assign!(microphone_id);
        assign!(improve_dictation_with_ai);
        assign!(dictation_language);
        assign!(sound_feedback);
        assign!(writing_shortcut);
        assign!(enabled_writing_actions);
        assign!(writing_popup_anchor);
        assign!(writing_popup_x);
        assign!(writing_popup_y);
        assign!(writing_popup_width);
        assign!(writing_popup_height);
        assign!(writing_allow_manual_text);
        assign!(onboarding_complete);
        settings
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    platform: &'static str,
    version: String,
    development: bool,
    paused: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPermissionStatus {
    kind: &'static str,
    state: &'static str,
    required: bool,
    explanation: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    configured: bool,
    connection: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechLanguage {
    pub code: String,
    pub name: String,
    pub installed: bool,
    pub downloadable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionContext {
    has_selection: bool,
    application_name: String,
    can_replace: bool,
    bounds: Option<ScreenRect>,
    initial_text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingRequest {
    action: String,
    instruction: Option<String>,
    /// Edited manual text-box content, quick-chat message, or explicit summary URL.
    text: Option<String>,
    #[serde(default)]
    source_kind: WritingSourceKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WritingResponse {
    Replaced,
    Result {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<LinkSource>,
        #[serde(rename = "canReplace")]
        can_replace: bool,
    },
}

#[tauri::command]
pub fn get_app_context(app: AppHandle, shell: State<'_, crate::shell::ShellState>) -> AppContext {
    AppContext {
        platform: if cfg!(target_os = "windows") {
            "windows"
        } else {
            "macos"
        },
        version: app.package_info().version.to_string(),
        development: cfg!(debug_assertions),
        paused: shell.paused(),
    }
}

#[tauri::command]
pub fn get_settings(core: State<'_, AppCore>) -> Result<FrontendSettings, CommandError> {
    core.settings().map(Into::into).map_err(Into::into)
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    core: State<'_, AppCore>,
    patch: SettingsPatch,
) -> Result<FrontendSettings, CommandError> {
    let current = FrontendSettings::from(core.settings().map_err(CommandError::from)?);
    let updated = patch.apply(current);
    let native_settings = updated.clone().try_into().map_err(CommandError::from)?;
    crate::shell::apply_settings(&app, &updated).map_err(CommandError::from)?;
    let saved = core
        .save_settings(native_settings)
        .map(FrontendSettings::from)
        .map_err(CommandError::from)?;
    let _ = app.emit("settings-changed", &saved);
    Ok(saved)
}

#[tauri::command]
pub fn get_permission_statuses(
    app: AppHandle,
    core: State<'_, AppCore>,
    platform: State<'_, Arc<crate::platform::PlatformServices>>,
) -> Result<Vec<FrontendPermissionStatus>, CommandError> {
    let statuses = permission_statuses(&platform).map_err(platform_command_error)?;
    refresh_shortcuts_after_permission(&app, &core, &statuses);
    Ok(statuses)
}

#[tauri::command]
pub fn request_permission(
    app: AppHandle,
    core: State<'_, AppCore>,
    platform: State<'_, Arc<crate::platform::PlatformServices>>,
    kind: String,
) -> Result<Vec<FrontendPermissionStatus>, CommandError> {
    let kind = parse_permission(&kind)?;
    platform
        .request_permission(kind)
        .map_err(platform_command_error)?;
    let statuses = permission_statuses(&platform).map_err(platform_command_error)?;
    refresh_shortcuts_after_permission(&app, &core, &statuses);
    let _ = app.emit("permission-status-changed", &statuses);
    Ok(statuses)
}

fn refresh_shortcuts_after_permission(
    app: &AppHandle,
    core: &AppCore,
    statuses: &[FrontendPermissionStatus],
) {
    let input_ready = statuses
        .iter()
        .any(|status| status.kind == "input-monitoring" && status.state == "granted");
    if input_ready && let Ok(settings) = core.settings().map(FrontendSettings::from) {
        let _ = crate::shell::register_shortcuts(app, &settings);
    }
}

#[tauri::command]
pub fn open_permission_settings(kind: String) -> Result<(), CommandError> {
    crate::shell::open_permission_settings(parse_permission(&kind)?).map_err(platform_command_error)
}

#[tauri::command]
pub async fn list_microphones(
    core: State<'_, AppCore>,
) -> Result<Vec<MicrophoneDevice>, CommandError> {
    core.microphones().await.map_err(Into::into)
}

#[tauri::command]
pub fn list_speech_languages() -> Vec<SpeechLanguage> {
    #[cfg(target_os = "windows")]
    if let Some(languages) = windows_speech_languages() {
        return languages;
    }
    vec![
        SpeechLanguage {
            code: "auto".into(),
            name: "Automatic".into(),
            installed: true,
            downloadable: false,
        },
        SpeechLanguage {
            code: "en-GB".into(),
            name: "English (United Kingdom)".into(),
            installed: true,
            downloadable: false,
        },
        SpeechLanguage {
            code: "en-US".into(),
            name: "English (United States)".into(),
            installed: true,
            downloadable: false,
        },
    ]
}

// The hardcoded list above is macOS-centric: on Windows only the installed
// speech packs work, and offering anything else hard-fails dictation at
// hotkey time (e.g. en-US without its pack: 0x800455BC). Query the real
// list; any failure falls back to the static list.
#[cfg(target_os = "windows")]
fn windows_speech_languages() -> Option<Vec<SpeechLanguage>> {
    use ::windows::{Globalization::Language, Media::SpeechRecognition::SpeechRecognizer};

    let supported = SpeechRecognizer::SupportedTopicLanguages().ok()?;
    let size = supported.Size().ok()?;
    let mut languages = Vec::with_capacity(size as usize + 1);
    languages.push(SpeechLanguage {
        code: "auto".into(),
        name: "Automatic".into(),
        installed: true,
        downloadable: false,
    });
    for index in 0..size {
        let language: Language = supported.GetAt(index).ok()?;
        let tag = language.LanguageTag().ok()?.to_string();
        if tag.is_empty()
            || languages
                .iter()
                .any(|existing: &SpeechLanguage| existing.code == tag)
        {
            continue;
        }
        let name = language
            .DisplayName()
            .map(|name| name.to_string())
            .unwrap_or_else(|_| tag.clone());
        languages.push(SpeechLanguage {
            code: tag,
            name,
            installed: true,
            downloadable: false,
        });
    }
    Some(languages)
}

#[tauri::command]
pub fn get_api_key_status(core: State<'_, AppCore>) -> Result<ApiKeyStatus, CommandError> {
    Ok(ApiKeyStatus {
        configured: core
            .credential_status()
            .map_err(CommandError::from)?
            .configured,
        connection: "untested",
    })
}

#[tauri::command]
pub fn store_api_key(
    core: State<'_, AppCore>,
    api_key: SecretString,
) -> Result<ApiKeyStatus, CommandError> {
    let status = core.save_api_key(api_key).map_err(CommandError::from)?;
    Ok(ApiKeyStatus {
        configured: status.configured,
        connection: "untested",
    })
}

#[tauri::command]
pub fn remove_api_key(core: State<'_, AppCore>) -> Result<ApiKeyStatus, CommandError> {
    let status = core.clear_api_key().map_err(CommandError::from)?;
    Ok(ApiKeyStatus {
        configured: status.configured,
        connection: "untested",
    })
}

#[tauri::command]
pub async fn test_api_key(core: State<'_, AppCore>) -> Result<ApiKeyStatus, CommandError> {
    let status = core.test_api_key().await.map_err(CommandError::from)?;
    Ok(ApiKeyStatus {
        configured: status.configured,
        connection: "connected",
    })
}

#[tauri::command]
pub async fn start_dictation(app: AppHandle, core: State<'_, AppCore>) -> Result<(), CommandError> {
    crate::shell::begin_dictation(&app, &core).await
}

#[tauri::command]
pub async fn stop_dictation(app: AppHandle, core: State<'_, AppCore>) -> Result<(), CommandError> {
    crate::shell::finish_dictation(&app, &core).await
}

#[tauri::command]
pub async fn cancel_dictation(
    app: AppHandle,
    core: State<'_, AppCore>,
) -> Result<(), CommandError> {
    core.cancel_dictation().await.map_err(CommandError::from)?;
    crate::shell::unregister_cancel_shortcut(&app);
    crate::shell::sync_idle_flow_bar(&app);
    Ok(())
}

#[tauri::command]
pub async fn retry_dictation(app: AppHandle, core: State<'_, AppCore>) -> Result<(), CommandError> {
    let _ = core.cancel_dictation().await;
    crate::shell::begin_dictation(&app, &core).await
}

#[tauri::command]
pub async fn get_writing_context(
    app: AppHandle,
    core: State<'_, AppCore>,
) -> Result<SelectionContext, CommandError> {
    let context = if core.writing_phase().map_err(CommandError::from)? == WritingPopupPhase::Ready {
        core.writing_context().map_err(CommandError::from)?
    } else {
        core.open_writing_tools()
            .await
            .map_err(CommandError::from)?
    };
    crate::shell::position_writing_surface(&app, context.cursor, context.anchor);
    let _ = app
        .get_webview_window("writing-tools")
        .and_then(|window| window.set_focus().ok().map(|_| window));
    Ok(selection_context(context))
}

#[tauri::command]
pub async fn run_writing_action(
    core: State<'_, AppCore>,
    request: WritingRequest,
) -> Result<WritingResponse, CommandError> {
    let action = parse_action(&request.action)?;
    match core
        .run_writing_action(
            action,
            request.instruction,
            request.text,
            request.source_kind,
        )
        .await
        .map_err(CommandError::from)?
    {
        WritingOutcome::Replaced => Ok(WritingResponse::Replaced),
        WritingOutcome::Result {
            markdown,
            source,
            can_replace,
        } => Ok(WritingResponse::Result {
            text: markdown,
            source,
            can_replace,
        }),
    }
}

#[tauri::command]
pub async fn replace_writing_result(
    core: State<'_, AppCore>,
    text: String,
) -> Result<(), CommandError> {
    let _ = text;
    core.replace_with_last_result()
        .await
        .map_err(CommandError::from)?;
    Ok(())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), CommandError> {
    crate::shell::copy_text(&text).map_err(platform_command_error)
}

#[tauri::command]
pub fn close_surface(
    app: AppHandle,
    surface: String,
    core: State<'_, AppCore>,
) -> Result<(), CommandError> {
    if surface == "writing-tools" {
        let _ = core.dismiss_writing_tools();
    }
    crate::shell::hide_surface(&app, &surface);
    Ok(())
}

#[tauri::command]
pub async fn show_surface(app: AppHandle, surface: String) -> Result<(), CommandError> {
    if surface == "writing-tools" {
        return crate::shell::open_writing_tools(&app).await;
    }
    crate::shell::show_surface(&app, &surface, true).map_err(platform_command_error)
}

#[tauri::command]
pub fn set_surface_mode(
    app: AppHandle,
    core: State<'_, AppCore>,
    surface: String,
    mode: String,
) -> Result<(), CommandError> {
    if surface != "writing-tools" {
        return Err(CommandError {
            code: "invalid_surface".into(),
            message: "That surface cannot be resized.".into(),
            recoverable: false,
        });
    }
    crate::shell::size_writing_surface(&app, &mode).map_err(platform_command_error)?;
    if let Ok(context) = core.writing_context() {
        crate::shell::position_writing_surface(&app, context.cursor, context.anchor);
    }
    Ok(())
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle) -> Result<(), CommandError> {
    crate::shell::hide_surface(&app, "onboarding");
    crate::shell::show_surface(&app, "settings", true).map_err(platform_command_error)
}

#[tauri::command]
pub fn set_paused(shell: State<'_, crate::shell::ShellState>, paused: bool) {
    shell.set_paused(paused);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub(crate) current_version: String,
    pub(crate) available: bool,
    pub(crate) available_version: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateResult, CommandError> {
    crate::shell::check_for_updates(&app).await
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), CommandError> {
    crate::shell::open_external(&app, &url).map_err(platform_command_error)
}

fn selection_context(context: WritingPopupContext) -> SelectionContext {
    SelectionContext {
        has_selection: context.has_selection,
        application_name: context.application.display_name,
        can_replace: context.has_selection,
        bounds: context.anchor,
        initial_text: context.initial_text,
    }
}

fn permission_statuses(
    platform: &crate::platform::PlatformServices,
) -> Result<Vec<FrontendPermissionStatus>, crate::platform::PlatformError> {
    [
        (
            crate::platform::PermissionKind::Accessibility,
            "accessibility",
            true,
            "Read and replace selected text.",
        ),
        (
            crate::platform::PermissionKind::InputMonitoring,
            "input-monitoring",
            cfg!(target_os = "macos"),
            "Detect the Fn hold shortcut.",
        ),
        (
            crate::platform::PermissionKind::Microphone,
            "microphone",
            true,
            "Listen while dictation is active.",
        ),
        (
            crate::platform::PermissionKind::SpeechRecognition,
            "speech-recognition",
            cfg!(target_os = "macos"),
            "Transcribe speech using the operating system.",
        ),
    ]
    .into_iter()
    .map(|(permission, kind, required, explanation)| {
        platform
            .permission_status(permission)
            .map(|status| FrontendPermissionStatus {
                kind,
                state: permission_state(status),
                required,
                explanation,
            })
    })
    .collect()
}

fn parse_permission(value: &str) -> Result<crate::platform::PermissionKind, CommandError> {
    match value {
        "accessibility" => Ok(crate::platform::PermissionKind::Accessibility),
        "input-monitoring" => Ok(crate::platform::PermissionKind::InputMonitoring),
        "microphone" => Ok(crate::platform::PermissionKind::Microphone),
        "speech-recognition" => Ok(crate::platform::PermissionKind::SpeechRecognition),
        _ => Err(CommandError {
            code: "invalid_permission".into(),
            message: "That permission is not supported.".into(),
            recoverable: false,
        }),
    }
}

fn permission_state(status: crate::platform::PermissionStatus) -> &'static str {
    match status {
        crate::platform::PermissionStatus::Granted => "granted",
        crate::platform::PermissionStatus::Denied
        | crate::platform::PermissionStatus::Restricted => "denied",
        crate::platform::PermissionStatus::NotDetermined => "not-determined",
        crate::platform::PermissionStatus::Unavailable => "unavailable",
    }
}

fn parse_action(value: &str) -> Result<WritingAction, AppCoreError> {
    match value {
        "proofread" => Ok(WritingAction::Proofread),
        "rewrite" => Ok(WritingAction::Rewrite),
        "friendly" => Ok(WritingAction::Friendly),
        "professional" => Ok(WritingAction::Professional),
        "concise" => Ok(WritingAction::Concise),
        "custom" => Ok(WritingAction::Custom),
        "summarize" => Ok(WritingAction::Summarize),
        "key-points" => Ok(WritingAction::KeyPoints),
        "chat" => Ok(WritingAction::Chat),
        _ => Err(AppCoreError::ActionDisabled),
    }
}

fn action_id(action: WritingAction) -> &'static str {
    match action {
        WritingAction::Proofread => "proofread",
        WritingAction::Rewrite => "rewrite",
        WritingAction::Friendly => "friendly",
        WritingAction::Professional => "professional",
        WritingAction::Concise => "concise",
        WritingAction::Custom => "custom",
        WritingAction::Summarize => "summarize",
        WritingAction::KeyPoints => "key-points",
        WritingAction::Chat => "chat",
    }
}

fn platform_command_error(error: crate::platform::PlatformError) -> CommandError {
    CommandError {
        code: format!("platform_{:?}", error.kind).to_lowercase(),
        message: error.message,
        recoverable: matches!(
            error.kind,
            crate::platform::PlatformErrorKind::Speech | crate::platform::PlatformErrorKind::Os
        ),
    }
}

fn invalid_settings_json() -> serde_json::Error {
    serde_json::from_str::<serde_json::Value>("{").unwrap_err()
}

#[cfg(test)]
mod tests;
