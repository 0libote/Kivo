use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{
    ai::{
        AiPrompt, AiProvider, AiReasoningMode, AiTaskKind, GeminiClient, GeminiError, LinkSource,
        ListedAiModel, OpenAiCompatClient, OpencodeError, PromptError, WritingAction,
        canonical_model_id_for, curated_models_for, dictation_cleanup_prompt, writing_prompt,
        writing_prompt_from_system,
    },
    config::{
        AppSettings, LanguagePreference, SettingsError, SettingsRepository, SettingsRuntime,
        SettingsRuntimeError, SpeechEnginePreference,
    },
    security::{CredentialError, CredentialStatus, CredentialStore, SecretString},
    speech::{
        DictationMachine, DictationPhase, MicrophoneDevice, SpeechBackend, SpeechEngine,
        SpeechError, SpeechEventSink, SpeechStartOptions,
        model_store::{LocalSpeechModel, ModelStore, ModelStoreError},
    },
    text::{
        ActiveApplication, CapturedSelection, ScreenPoint, ScreenRect, TextError, TextService,
        WritingPopupMachine, WritingPopupPhase, WritingTransitionError,
    },
};

const DICTATION_AI_DEADLINE: Duration = Duration::from_secs(4);
const WRITING_AI_DEADLINE: Duration = Duration::from_secs(20);
const LINK_SUMMARY_AI_DEADLINE: Duration = Duration::from_secs(60);

pub struct AppCore {
    settings: RwLock<AppSettings>,
    settings_repository: SettingsRepository,
    settings_runtime: Arc<dyn SettingsRuntime>,
    credentials: Arc<dyn CredentialStore>,
    ai: GeminiClient,
    compat: OpenAiCompatClient,
    speech: Arc<dyn SpeechEngine>,
    text: Arc<dyn TextService>,
    dictation: Mutex<DictationMachine>,
    dictation_operation: tokio::sync::Mutex<()>,
    dictation_generation: AtomicU64,
    dictation_cancel: tokio::sync::watch::Sender<()>,
    dictation_target: Mutex<Option<Box<dyn crate::text::InsertionTarget>>>,
    last_transcript: Mutex<Option<String>>,
    writing: Mutex<WritingPopupMachine>,
    pending_selection: Mutex<Option<Arc<CapturedSelection>>>,
    last_result: Mutex<Option<StoredWritingResult>>,
    last_cursor: Mutex<Option<ScreenPoint>>,
    writing_generation: AtomicU64,
    writing_cancel: tokio::sync::watch::Sender<()>,
    /// Local AI usage ledger (counts and identifiers only).
    usage: Arc<crate::usage::UsageStore>,
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
        let compat = OpenAiCompatClient::new()?;
        Ok(Self {
            settings: RwLock::new(settings),
            settings_repository,
            settings_runtime,
            credentials,
            ai,
            compat,
            speech,
            text,
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
            usage: crate::usage::UsageStore::in_memory(),
        })
    }

    /// Attach the persistent usage ledger to the AI clients. Called once at
    /// startup; tests and headless setups keep the in-memory default so
    /// generation behaviour is unchanged.
    pub fn attach_usage_store(&mut self, usage: Arc<crate::usage::UsageStore>) {
        self.ai.set_usage_store(Arc::clone(&usage));
        self.compat.set_usage_store(Arc::clone(&usage));
        self.usage = usage;
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
            // Restore the previous runtime state. A failing restore cannot
            // reintroduce `settings` (see ShellSettingsRuntime::apply);
            // shortcuts and theme are re-applied from the saved file on next
            // launch, so any residual OS-side divergence is transient.
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
        let provider = self.ai_provider();
        self.credentials
            .status(provider.credential_account())
            .map_err(Into::into)
    }

    pub fn save_api_key(&self, api_key: SecretString) -> Result<CredentialStatus, AppCoreError> {
        let provider = self.ai_provider();
        self.credentials
            .save_api_key(provider.credential_account(), &api_key)?;
        Ok(CredentialStatus { configured: true })
    }

    pub fn clear_api_key(&self) -> Result<CredentialStatus, AppCoreError> {
        let provider = self.ai_provider();
        self.credentials
            .clear_api_key(provider.credential_account())?;
        Ok(CredentialStatus { configured: false })
    }

    fn load_provider_key(
        &self,
        provider: AiProvider,
    ) -> Result<Option<SecretString>, AppCoreError> {
        self.credentials
            .load_api_key(provider.credential_account())
            .map_err(Into::into)
    }

    fn require_provider_key(&self, provider: AiProvider) -> Result<SecretString, AppCoreError> {
        // Custom targets local servers first: no stored key is fine there.
        if provider.key_optional() {
            if let Some(key) = self.load_provider_key(provider)? {
                return Ok(key);
            }
            return Err(AppCoreError::AiNotConfigured { provider });
        }
        self.load_provider_key(provider)?
            .ok_or(AppCoreError::AiNotConfigured { provider })
    }

    pub async fn test_api_key(&self) -> Result<CredentialStatus, AppCoreError> {
        let (provider, models, base_url, _) = self.ai_config();
        match provider {
            AiProvider::Gemini => {
                let api_key = self.require_provider_key(provider)?;
                // Model listings include models that may reject generation for
                // this account. Exercise the selected model before reporting ready.
                let first = models
                    .first()
                    .map(String::as_str)
                    .unwrap_or(crate::ai::DEFAULT_GEMINI_MODEL);
                self.ai
                    .generate(
                        &api_key,
                        first,
                        &AiPrompt {
                            input: "Reply with OK.".into(),
                            system_instruction: "Return only OK.".into(),
                            kind: AiTaskKind::ConnectionTest,
                        },
                        AiReasoningMode::Fast,
                    )
                    .await?;
                if models.len() > 1 {
                    let listed = self.ai.list_models(&api_key).await?;
                    if let Some(missing) = find_unavailable_model(&listed, provider, &models[1..]) {
                        return Err(AppCoreError::AiModelUnavailable { model: missing });
                    }
                }
                Ok(CredentialStatus { configured: true })
            }
            AiProvider::Zen | AiProvider::Go | AiProvider::Custom => {
                // Zen/Go list their models publicly, so a metadata GET cannot
                // validate the key: send a tiny completion on the first
                // queue entry (a minimal authenticated call), then
                // verify the remaining entries against the public listing
                // without spending further quota.
                let chat_url = provider
                    .resolve_chat_url(base_url.as_deref())
                    .ok_or(AppCoreError::AiNotConfigured { provider })?;
                let api_key = if provider.key_optional() {
                    self.load_provider_key(provider)?
                } else {
                    Some(self.require_provider_key(provider)?)
                };
                let first = models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| crate::ai::DEFAULT_GEMINI_MODEL.to_owned());
                self.compat
                    .test_connection(provider, &chat_url, api_key.as_ref(), &first)
                    .await?;
                if models.len() > 1 {
                    let listed = self
                        .list_provider_models(provider, base_url.as_deref())
                        .await;
                    if let Some(missing) = find_unavailable_model(&listed, provider, &models[1..]) {
                        return Err(AppCoreError::AiModelUnavailable { model: missing });
                    }
                }
                Ok(CredentialStatus { configured: true })
            }
        }
    }

    fn ai_provider(&self) -> AiProvider {
        self.settings()
            .map(|settings| settings.ai.provider)
            .unwrap_or_default()
    }

    fn ai_config(&self) -> (AiProvider, Vec<String>, Option<String>, AiReasoningMode) {
        self.settings()
            .map(|settings| {
                (
                    settings.ai.provider,
                    settings.ai.models,
                    settings.ai.custom_base_url,
                    settings.ai.reasoning_mode,
                )
            })
            .unwrap_or_else(|_| {
                (
                    AiProvider::default(),
                    vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()],
                    None,
                    AiReasoningMode::default(),
                )
            })
    }

    /// Synchronous key load for the model picker. Returns `None` when no key
    /// is stored or it cannot be read; the caller falls back to curated
    /// suggestions instead of surfacing an error.
    fn stored_api_key_for_models(&self, provider: AiProvider) -> Option<SecretString> {
        self.load_provider_key(provider).ok().flatten()
    }

    /// Generate text on the configured provider, trying each queued model in
    /// order until one succeeds (see the clients' `generate_in_order`).
    async fn generate_text(
        &self,
        provider: AiProvider,
        api_key: Option<&SecretString>,
        models: &[String],
        base_url: Option<&str>,
        prompt: &AiPrompt,
        reasoning_mode: AiReasoningMode,
    ) -> Result<String, AppCoreError> {
        match provider {
            AiProvider::Gemini => {
                let api_key = api_key.ok_or(AppCoreError::AiNotConfigured { provider })?;
                self.ai
                    .generate_in_order(api_key, models, prompt, reasoning_mode)
                    .await
                    .map_err(Into::into)
            }
            AiProvider::Zen | AiProvider::Go | AiProvider::Custom => {
                let chat_url = provider
                    .resolve_chat_url(base_url)
                    .ok_or(AppCoreError::AiNotConfigured { provider })?;
                self.compat
                    .generate_in_order(provider, &chat_url, api_key, models, prompt, reasoning_mode)
                    .await
                    .map_err(Into::into)
            }
        }
    }

    /// List models on the configured provider. Dynamic source of truth with
    /// a curated fallback so the selector never appears empty.
    async fn list_provider_models(
        &self,
        provider: AiProvider,
        base_url: Option<&str>,
    ) -> Vec<ListedAiModel> {
        match provider {
            AiProvider::Gemini => {
                let api_key = self.stored_api_key_for_models(provider);
                match api_key {
                    Some(api_key) => match self.ai.list_models(&api_key).await {
                        Ok(models) if !models.is_empty() => models,
                        _ => crate::ai::curated_listed_models(),
                    },
                    None => crate::ai::curated_listed_models(),
                }
            }
            AiProvider::Zen | AiProvider::Go | AiProvider::Custom => {
                let models_url = provider.resolve_models_url(base_url);
                let api_key = self.stored_api_key_for_models(provider);
                match models_url {
                    Some(models_url) => match self
                        .compat
                        .list_models(provider, &models_url, api_key.as_ref())
                        .await
                    {
                        Ok(models) if !models.is_empty() => models,
                        _ => curated_models_for(provider),
                    },
                    None => curated_models_for(provider),
                }
            }
        }
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

    /// Aggregated AI usage newer than `since_ms` (all entries when `None`).
    pub fn usage_summary(&self, since_ms: Option<i64>) -> crate::usage::UsageSummary {
        self.usage.summary(since_ms)
    }

    /// Delete the local usage ledger.
    pub fn clear_usage_stats(&self) {
        self.usage.clear();
    }

    pub async fn microphones(&self) -> Result<Vec<MicrophoneDevice>, AppCoreError> {
        self.speech.microphones().await.map_err(Into::into)
    }

    pub fn dictation_generation(&self) -> u64 {
        self.dictation_generation.load(Ordering::Acquire)
    }

    pub fn recovery_text(&self) -> Result<Option<String>, AppCoreError> {
        Ok(self
            .last_transcript
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .clone())
    }

    pub fn clear_recovery(&self) -> Result<(), AppCoreError> {
        self.last_transcript
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .take();
        Ok(())
    }

    pub async fn begin_dictation(
        &self,
        events: SpeechEventSink,
    ) -> Result<DictationPhase, AppCoreError> {
        let _operation = self
            .dictation_operation
            .try_lock()
            .map_err(|_| SpeechError::AlreadyRunning)?;
        if matches!(
            self.dictation_phase()?,
            DictationPhase::Starting | DictationPhase::Listening | DictationPhase::Processing
        ) {
            return Err(SpeechError::AlreadyRunning.into());
        }
        // Capture before any asynchronous work or window activation. A missing
        // target still allows speech, but delivery becomes explicit recovery.
        let target = self.text.capture_insertion_target().ok();
        let generation = self.dictation_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let mut cancelled = self.dictation_cancel.subscribe();
        self.dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .prepare()?;
        let settings = self.settings()?.dictation;
        let locale = match settings.language {
            LanguagePreference::Auto => None,
            LanguagePreference::Locale { tag } => Some(tag),
        };
        let backend = match settings.speech_engine {
            SpeechEnginePreference::System => SpeechBackend::System,
            SpeechEnginePreference::Local => SpeechBackend::Local {
                model_id: settings.local_speech_model.clone().unwrap_or_default(),
            },
        };
        let started = tokio::select! {
            biased;
            _ = cancelled.changed() => return Ok(DictationPhase::Hidden),
            result = self.speech.start(SpeechStartOptions { microphone_id: settings.microphone_id, locale, backend }, events) => result,
        };
        let session = match started {
            Ok(session) => session,
            Err(error) => {
                self.fail_dictation(generation);
                return Err(error.into());
            }
        };
        let phase = {
            let mut machine = self
                .dictation
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?;
            if self.dictation_generation() == generation
                && machine.phase() == DictationPhase::Starting
            {
                *self
                    .dictation_target
                    .lock()
                    .map_err(|_| AppCoreError::Unavailable)? = target;
                machine.begin(session)?;
                Some(machine.phase())
            } else {
                None
            }
        };
        if let Some(phase) = phase {
            Ok(phase)
        } else {
            let _ = self.speech.cancel(session).await;
            Ok(DictationPhase::Hidden)
        }
    }

    pub async fn finish_dictation(&self) -> Result<DictationPhase, AppCoreError> {
        let _operation = self
            .dictation_operation
            .try_lock()
            .map_err(|_| SpeechError::AlreadyRunning)?;
        let generation = self.dictation_generation();
        let mut cancelled = self.dictation_cancel.subscribe();
        let session = self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .release()?;
        let transcript = tokio::select! {
            biased;
            _ = cancelled.changed() => return Ok(DictationPhase::Hidden),
            result = self.speech.stop(session) => result,
        };
        let transcript = match transcript {
            Ok(transcript) => transcript,
            Err(error) => {
                self.fail_dictation(generation);
                return Err(error.into());
            }
        };
        let settings = self.settings()?;
        let ai_provider = settings.ai.provider;
        let ai_models = settings.ai.models.clone();
        let ai_base_url = settings.ai.custom_base_url.clone();
        let dictation = settings.dictation;
        // A dedicated cleanup model pins the request to one entry; otherwise
        // cleanup follows the Writing Tools failover queue.
        let cleanup_models = match dictation.cleanup_model.clone() {
            Some(model) => vec![model],
            None => ai_models,
        };
        let mut final_text = transcript.into_text();
        if dictation.improve_with_ai
            && let Ok(prompt) = dictation_cleanup_prompt(&final_text)
        {
            // Missing keys fail silently here (raw transcript is kept): dictation
            // cleanup is best-effort inside a 4s deadline, never a hard error.
            // Custom targets local servers, so no stored key is fine there.
            let api_key = self.load_provider_key(ai_provider).ok().flatten();
            if api_key.is_some() || ai_provider.key_optional() {
                let cleaned = tokio::select! {
                    biased;
                    _ = cancelled.changed() => return Ok(DictationPhase::Hidden),
                    result = tokio::time::timeout(DICTATION_AI_DEADLINE, self.generate_text(ai_provider, api_key.as_ref(), &cleanup_models, ai_base_url.as_deref(), &prompt, AiReasoningMode::Fast)) => result,
                };
                if let Ok(Ok(cleaned)) = cleaned {
                    final_text = cleaned;
                }
            }
        }
        // Serialize the final check and synchronous native delivery with cancel.
        let mut machine = self
            .dictation
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?;
        if self.dictation_generation() != generation
            || machine.phase() != DictationPhase::Processing
        {
            return Ok(DictationPhase::Hidden);
        }
        *self
            .last_transcript
            .lock()
            .map_err(|_| AppCoreError::Unavailable)? = Some(final_text.clone());
        let target = self
            .dictation_target
            .lock()
            .map_err(|_| AppCoreError::Unavailable)?
            .take();
        if let Err(error) = target
            .ok_or(TextError::SelectionExpired)
            .and_then(|target| target.insert(&final_text))
        {
            machine.fail();
            return Err(error.into());
        }
        machine.complete()?;
        Ok(machine.phase())
    }

    pub async fn cancel_dictation(&self) -> Result<DictationPhase, AppCoreError> {
        let session = {
            let mut machine = self
                .dictation
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?;
            self.dictation_generation.fetch_add(1, Ordering::AcqRel);
            self.dictation_cancel.send_replace(());
            self.dictation_target
                .lock()
                .map_err(|_| AppCoreError::Unavailable)?
                .take();
            machine.cancel()
        };
        if let Some(session) = session {
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
        // Native capture normally resolves in milliseconds, but a hung
        // AX/UIA query must not hang the shortcut forever: time out so the
        // popup can show a failure instead of never opening.
        let captured = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.text.capture_selection(),
        )
        .await
        {
            Ok(captured) => captured,
            Err(_) => Err(crate::text::TextError::Backend),
        };
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
                WritingPopupContext::without_selection(cursor)
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
            // No selection (or a cleared context): the popup opens the
            // summarize entry so pasted text or a link can still be supplied.
            None => Ok(WritingPopupContext::without_selection(cursor)),
        }
    }

    /// Run a built-in action with its default prompt. Kept for tests that
    /// predate editable presets; the command layer uses
    /// [`Self::run_writing_action_with`].
    #[allow(dead_code, clippy::too_many_arguments)]
    pub async fn run_writing_action(
        &self,
        action: WritingAction,
        custom_instruction: Option<String>,
        source_override: Option<String>,
        source_kind: WritingSourceKind,
    ) -> Result<WritingOutcome, AppCoreError> {
        self.run_writing_action_with(
            action,
            None,
            custom_instruction,
            source_override,
            source_kind,
            None,
            None,
            None,
        )
        .await
    }

    /// Full Writing Tools invocation used by the command layer. User-editable
    /// presets resolve their prompt in the frontend and pass it as
    /// `system_instruction`; `preset_id` is the id used for the enabled check
    /// (custom presets reuse [`WritingAction::Custom`]). `models_override`
    /// pins a per-preset model priority instead of the global queue.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_writing_action_with(
        &self,
        action: WritingAction,
        preset_id: Option<&str>,
        custom_instruction: Option<String>,
        source_override: Option<String>,
        source_kind: WritingSourceKind,
        system_instruction: Option<String>,
        replaces_selection: Option<bool>,
        models_override: Option<Vec<String>>,
    ) -> Result<WritingOutcome, AppCoreError> {
        let enabled_id = preset_id.unwrap_or_else(|| action.as_str());
        if !self
            .settings()?
            .writing_tools
            .enabled_actions
            .iter()
            .any(|id| id == enabled_id)
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
        let ai_deadline = if source_kind == WritingSourceKind::Link {
            LINK_SUMMARY_AI_DEADLINE
        } else {
            WRITING_AI_DEADLINE
        };
        let generate = async {
            let source_text = source_override
                .as_deref()
                .or_else(|| selection.as_ref().map(|value| value.text()))
                .ok_or(PromptError::EmptySource)?;
            let (provider, mut models, base_url, reasoning_mode) = self.ai_config();
            if let Some(override_models) = models_override.as_ref()
                && !override_models.is_empty()
            {
                models = override_models.clone();
            }
            if source_kind == WritingSourceKind::Link {
                let source = LinkSource::parse(source_text)?;
                // Link retrieval (URL context / video input) is a Gemini
                // Interactions API capability. Other providers summarize pasted
                // text normally; for links they get the paste-instead guidance.
                if provider != AiProvider::Gemini {
                    return Err(AppCoreError::Opencode(
                        crate::ai::OpencodeError::InaccessibleSource,
                    ));
                }
                let api_key = self.require_provider_key(provider)?;
                let result = self
                    .ai
                    .summarize_link_in_order(&api_key, &models, &source, reasoning_mode)
                    .await?;
                Ok::<_, AppCoreError>((result, Some(source)))
            } else {
                let prompt = match system_instruction.as_deref() {
                    Some(instruction) if !instruction.trim().is_empty() => {
                        // `writing_prompt` enforces this for built-ins; the
                        // editable-template path must too.
                        if action == WritingAction::Summarize
                            && source_text.chars().count() > 200_000
                        {
                            return Err(PromptError::SummaryTooLong.into());
                        }
                        writing_prompt_from_system(instruction.to_owned(), source_text)?
                    }
                    _ => writing_prompt(action, source_text, custom_instruction.as_deref())?,
                };
                let api_key = if provider.key_optional() {
                    self.load_provider_key(provider)?
                } else {
                    Some(self.require_provider_key(provider)?)
                };
                let result = self
                    .generate_text(
                        provider,
                        api_key.as_ref(),
                        &models,
                        base_url.as_deref(),
                        &prompt,
                        reasoning_mode,
                    )
                    .await?;
                Ok((result, None))
            }
        };
        let generated = tokio::select! {
            biased;
            _ = cancellation.changed() => return Err(AppCoreError::WritingCancelled),
            result = tokio::time::timeout(ai_deadline, generate) => match result {
                Ok(result) => result,
                Err(_) => Err(AppCoreError::AiTimeout),
            },
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
        if replaces_selection.unwrap_or_else(|| action.replaces_selection()) {
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
            if replacement.is_err() {
                machine.recover_result()?;
                *self
                    .last_result
                    .lock()
                    .map_err(|_| AppCoreError::Unavailable)? = Some(StoredWritingResult {
                    text: Arc::from(result.as_str()),
                    // Keep the captured selection available for an explicit
                    // retry. Automatic replacement can fail for an editor's
                    // native text interface; making the result copy-only
                    // strands the user in a manual copy/paste flow.
                    can_replace: selection.is_some(),
                });
                return Ok(WritingOutcome::Result {
                    markdown: result,
                    source: None,
                    can_replace: selection.is_some(),
                });
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

    fn fail_dictation(&self, generation: u64) {
        if let Ok(mut machine) = self.dictation.lock()
            && self.dictation_generation() == generation
        {
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

/// First queue entry missing from a successful list result, if any. A missing
/// id means it is retired or not enabled for the key's project/tier (the key
/// itself is valid: listing succeeded), so Test connection reports "pick
/// another model" instead of "bad key".
fn find_unavailable_model(
    models: &[ListedAiModel],
    provider: AiProvider,
    queue: &[String],
) -> Option<String> {
    queue
        .iter()
        .map(|id| canonical_model_id_for(provider, id))
        .find(|id| !models.iter().any(|model| model.id == *id))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingPopupContext {
    pub application: ActiveApplication,
    pub anchor: Option<ScreenRect>,
    pub cursor: Option<ScreenPoint>,
    pub has_selection: bool,
    /// Text shown in the manual text box: the captured highlight, or empty
    /// when nothing is selected. Only sent to Kivo's own popup.
    pub initial_text: String,
}

impl WritingPopupContext {
    fn without_selection(cursor: Option<ScreenPoint>) -> Self {
        Self {
            application: ActiveApplication {
                identifier: "no-selection".into(),
                display_name: "Writing Tools".into(),
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
    AiNotConfigured { provider: AiProvider },
    AiModelUnavailable { model: String },
    ActionDisabled,
    SelectionExpired,
    NoResult,
    WritingCancelled,
    AiTimeout,
    Settings(SettingsError),
    SettingsRuntime(SettingsRuntimeError),
    Credential(CredentialError),
    Gemini(GeminiError),
    Opencode(OpencodeError),
    Prompt(PromptError),
    Speech(SpeechError),
    ModelStore(ModelStoreError),
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
            Self::Opencode(error) => Some(error),
            Self::Prompt(error) => Some(error),
            Self::Speech(error) => Some(error),
            Self::ModelStore(error) => Some(error),
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
            Self::AiNotConfigured { .. } => "ai_not_configured",
            Self::AiModelUnavailable { .. } => "model_unavailable",
            Self::ActionDisabled => "action_disabled",
            Self::SelectionExpired => "selection_expired",
            Self::NoResult => "no_result",
            Self::WritingCancelled => "writing_cancelled",
            Self::AiTimeout => "ai_timeout",
            Self::Settings(_) => "settings",
            Self::SettingsRuntime(_) => "settings_runtime",
            Self::Credential(_) => "credential",
            Self::Gemini(error) => error.code(),
            Self::Opencode(error) => error.code(),
            Self::Prompt(_) => "invalid_prompt",
            Self::Speech(_) => "speech",
            Self::ModelStore(_) => "local_model",
            Self::DictationTransition(_) => "dictation_state",
            Self::Text(_) => "text_integration",
            Self::WritingTransition(_) => "writing_state",
        }
    }

    pub(crate) fn user_message(&self) -> String {
        match self {
            Self::Unavailable => "The application is temporarily unavailable.".into(),
            Self::AiNotConfigured { provider } => match provider {
                AiProvider::Gemini => "Add your Google AI Studio API key first.".into(),
                AiProvider::Zen | AiProvider::Go => "Add your OpenCode API key first.".into(),
                AiProvider::Custom => {
                    "The custom endpoint couldn't be reached. Check the base URL.".into()
                }
            },
            Self::AiModelUnavailable { model } => format!(
                "Your API key works, but \"{model}\" isn't available to it. Choose another model under AI → Model."
            ),
            Self::ActionDisabled => "That writing action is disabled in Settings.".into(),
            Self::SelectionExpired => "The original selection is no longer available.".into(),
            Self::NoResult => "There is no result to replace the selection with.".into(),
            Self::WritingCancelled => "The writing request was cancelled.".into(),
            Self::AiTimeout => {
                "The AI took too long to respond. Try again or choose a faster model.".into()
            }
            Self::Gemini(error) => error.user_message(),
            Self::Opencode(error) => error.user_message(),
            Self::Settings(error) => error.to_string(),
            Self::SettingsRuntime(error) => error.to_string(),
            Self::Credential(error) => error.to_string(),
            Self::Prompt(error) => error.to_string(),
            Self::Speech(error) => error.to_string(),
            Self::ModelStore(error) => error.to_string(),
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
from_core_error!(Opencode, OpencodeError);
from_core_error!(Prompt, PromptError);
from_core_error!(Speech, SpeechError);
from_core_error!(ModelStore, ModelStoreError);
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
            | AppCoreError::Opencode(OpencodeError::Transport(_))
            | AppCoreError::Opencode(OpencodeError::EmptyResponse)
            | AppCoreError::Speech(SpeechError::NoSpeechDetected)
            | AppCoreError::Speech(SpeechError::RecognitionUnavailable)
            | AppCoreError::Speech(SpeechError::LocalModelUnavailable)
            | AppCoreError::ModelStore(_)
            | AppCoreError::Speech(SpeechError::Backend) => true,
            AppCoreError::AiTimeout => true,
            AppCoreError::Gemini(error) if error.is_rate_limited() => true,
            AppCoreError::Opencode(error) if error.is_rate_limited() => true,
            AppCoreError::Gemini(error) if error.is_server_error() => true,
            _ => false,
        };
        Self {
            code: error.code().into(),
            message: error.user_message(),
            recoverable,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_hold_threshold_ms() -> u64 {
    350
}

fn default_speech_engine() -> String {
    "system".into()
}

fn speech_engine_id(preference: SpeechEnginePreference) -> &'static str {
    match preference {
        SpeechEnginePreference::System => "system",
        SpeechEnginePreference::Local => "local",
    }
}

fn parse_speech_engine(value: &str) -> SpeechEnginePreference {
    match value {
        "local" => SpeechEnginePreference::Local,
        _ => SpeechEnginePreference::System,
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
    #[serde(default = "default_true")]
    pub dictation_tap_enabled: bool,
    #[serde(default = "default_true")]
    pub dictation_hold_enabled: bool,
    #[serde(default = "default_hold_threshold_ms")]
    pub dictation_hold_threshold_ms: u64,
    #[serde(default = "default_speech_engine")]
    pub speech_engine: String,
    #[serde(default)]
    pub local_speech_model: Option<String>,
    #[serde(default)]
    pub dictation_cleanup_model: Option<String>,
    pub writing_shortcut: String,
    pub enabled_writing_actions: Vec<String>,
    #[serde(default)]
    pub writing_presets: Vec<crate::config::WritingPresetSettings>,
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_models")]
    pub ai_models: Vec<String>,
    #[serde(default = "default_ai_reasoning_mode")]
    pub ai_reasoning_mode: String,
    #[serde(default)]
    pub ai_custom_base_url: Option<String>,
    pub onboarding_complete: bool,
    /// Legacy primary/backup pair, deserialize-only: accepted on the way in
    /// when `ai_models` is absent (pre-queue payloads), never serialized
    /// back out.
    #[serde(default, skip_serializing)]
    pub ai_model: Option<String>,
    #[serde(default, skip_serializing)]
    pub ai_backup_model: Option<Option<String>>,
}

fn default_ai_provider() -> String {
    AiProvider::default().as_str().to_owned()
}

fn default_ai_models() -> Vec<String> {
    vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
}

fn default_ai_reasoning_mode() -> String {
    AiReasoningMode::default().as_str().to_owned()
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
            dictation_tap_enabled: settings.dictation.tap_enabled,
            dictation_hold_enabled: settings.dictation.hold_enabled,
            dictation_hold_threshold_ms: settings.dictation.hold_threshold_ms,
            speech_engine: speech_engine_id(settings.dictation.speech_engine).into(),
            local_speech_model: settings.dictation.local_speech_model,
            dictation_cleanup_model: settings.dictation.cleanup_model,
            writing_shortcut: settings.writing_tools.shortcut.accelerator,
            enabled_writing_actions: settings.writing_tools.enabled_actions,
            writing_presets: settings.writing_tools.presets,
            ai_provider: settings.ai.provider.as_str().into(),
            ai_models: settings.ai.models,
            ai_reasoning_mode: settings.ai.reasoning_mode.as_str().into(),
            ai_custom_base_url: settings.ai.custom_base_url,
            onboarding_complete: settings.general.onboarding_complete,
            ai_model: None,
            ai_backup_model: None,
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
        // Enabled ids are canonicalized (legacy `keyPoints` → `key-points`)
        // and deduped by `WritingToolsSettings::normalize`. Built-in and custom
        // ids share this list; unknown ids are dropped there.
        let enabled_actions = settings.enabled_writing_actions;
        // The queue normalizes itself (dedupe, drop unusable, cap, fall
        // back to the provider default) so old settings files and
        // forward-compat payloads never break AI requests. Pre-queue
        // payloads carrying only `aiModel`/`aiBackupModel` migrate into a
        // two-entry queue. Unknown provider strings fall back to Gemini.
        let ai_provider = AiProvider::parse(&settings.ai_provider);
        let ai_models = if settings.ai_models.is_empty() {
            let mut migrated = Vec::new();
            for candidate in [settings.ai_model, settings.ai_backup_model.unwrap_or(None)] {
                if let Some(candidate) = candidate
                    && !candidate.trim().is_empty()
                {
                    migrated.push(candidate);
                }
            }
            migrated
        } else {
            settings.ai_models
        };
        let ai_models = crate::ai::normalize_model_list(ai_provider, &ai_models);
        let ai_reasoning_mode = AiReasoningMode::parse(&settings.ai_reasoning_mode);
        let ai_custom_base_url =
            crate::ai::normalize_base_url(settings.ai_custom_base_url.as_deref());
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
                tap_enabled: settings.dictation_tap_enabled,
                hold_enabled: settings.dictation_hold_enabled,
                hold_threshold_ms: settings.dictation_hold_threshold_ms,
                speech_engine: parse_speech_engine(&settings.speech_engine),
                local_speech_model: settings.local_speech_model,
                cleanup_model: settings.dictation_cleanup_model,
            },
            writing_tools: crate::config::WritingToolsSettings {
                shortcut: crate::config::ShortcutBinding::new(settings.writing_shortcut),
                enabled_actions,
                presets: settings.writing_presets,
            },
            ai: crate::config::AiSettings::new(
                ai_provider,
                ai_models,
                ai_reasoning_mode,
                ai_custom_base_url,
            ),
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
    dictation_tap_enabled: Option<bool>,
    dictation_hold_enabled: Option<bool>,
    dictation_hold_threshold_ms: Option<u64>,
    speech_engine: Option<String>,
    local_speech_model: Option<Option<String>>,
    dictation_cleanup_model: Option<Option<String>>,
    writing_shortcut: Option<String>,
    enabled_writing_actions: Option<Vec<String>>,
    writing_presets: Option<Vec<crate::config::WritingPresetSettings>>,
    ai_provider: Option<String>,
    ai_models: Option<Vec<String>>,
    ai_reasoning_mode: Option<String>,
    ai_custom_base_url: Option<Option<String>>,
    onboarding_complete: Option<bool>,
}

impl SettingsPatch {
    fn apply(self, mut settings: FrontendSettings) -> FrontendSettings {
        if let Some(provider) = self.ai_provider.as_deref() {
            let provider = AiProvider::parse(provider);
            if provider != AiProvider::parse(&settings.ai_provider) && self.ai_models.is_none() {
                // Model IDs are provider-specific even when their syntax is valid
                // on both services. Keep an explicitly supplied queue, otherwise
                // start the new provider with its supported default.
                settings.ai_models = vec![provider.default_model().to_owned()];
            }
        }
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
        assign!(dictation_tap_enabled);
        assign!(dictation_hold_enabled);
        assign!(dictation_hold_threshold_ms);
        assign!(speech_engine);
        assign!(local_speech_model);
        assign!(dictation_cleanup_model);
        assign!(writing_shortcut);
        assign!(enabled_writing_actions);
        assign!(writing_presets);
        assign!(ai_provider);
        assign!(ai_models);
        assign!(ai_reasoning_mode);
        assign!(ai_custom_base_url);
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
    /// The preset that triggered the request, for the enabled check. Defaults
    /// to `action`; custom presets send their own id while `action` stays
    /// `custom` so the backend state machine has a built-in phase.
    #[serde(default)]
    preset_id: Option<String>,
    instruction: Option<String>,
    /// Fully resolved system prompt for user-editable presets. `None` falls
    /// back to the backend's built-in template for `action`.
    #[serde(default)]
    system_instruction: Option<String>,
    /// Overrides the built-in replace/show-result behavior.
    #[serde(default)]
    replaces_selection: Option<bool>,
    /// Per-preset model priority. Empty follows the global AI queue.
    #[serde(default)]
    models: Vec<String>,
    /// Edited manual text-box content, or the explicit summary text / URL.
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
        } else if cfg!(target_os = "linux") {
            "linux"
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

/// Clears Kivo's own TCC entries so a new build can be enabled when a stale
/// entry from a previous (ad-hoc signed) build is stuck in System Settings.
/// Scoped to Kivo's bundle id only: other apps' grants are never touched.
/// After the reset the user re-allows each permission; statuses are
/// re-read and broadcast like any other permission change.
#[tauri::command]
pub fn reset_permission_grants(
    app: AppHandle,
    core: State<'_, AppCore>,
    platform: State<'_, Arc<crate::platform::PlatformServices>>,
) -> Result<Vec<FrontendPermissionStatus>, CommandError> {
    reset_tcc_grants().map_err(platform_command_error)?;
    let statuses = permission_statuses(&platform).map_err(platform_command_error)?;
    refresh_shortcuts_after_permission(&app, &core, &statuses);
    let _ = app.emit("permission-status-changed", &statuses);
    Ok(statuses)
}

/// `tccutil reset All <bundle-id>` drops every TCC grant for Kivo's bundle
/// id (Accessibility, Input Monitoring, Microphone, Speech Recognition)
/// without touching other apps. Per-user database, so no sudo needed.
#[cfg(target_os = "macos")]
fn reset_tcc_grants() -> Result<(), crate::platform::PlatformError> {
    let output = std::process::Command::new("tccutil")
        .args(["reset", "All", "com.kivo.desktop"])
        .output()
        .map_err(|_| {
            crate::platform::PlatformError::new(
                crate::platform::PlatformErrorKind::Os,
                "reset_permission_grants",
                "Could not clear the old permission entries.",
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(crate::platform::PlatformError::new(
            crate::platform::PlatformErrorKind::Os,
            "reset_permission_grants",
            "Could not clear the old permission entries.",
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn reset_tcc_grants() -> Result<(), crate::platform::PlatformError> {
    Err(crate::platform::PlatformError::new(
        crate::platform::PlatformErrorKind::Unsupported,
        "reset_permission_grants",
        "Clearing permission entries is only available on macOS.",
    ))
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
    // macOS enumerates the OS voices elsewhere; Linux uses the simulated
    // test-bench engine. Same shape on every host so the picker never
    // appears empty and selection code paths stay identical.
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

#[cfg(target_os = "windows")]
fn windows_speech_languages() -> Option<Vec<SpeechLanguage>> {
    let mut languages = vec![SpeechLanguage {
        code: "auto".into(),
        name: "System default".into(),
        installed: true,
        downloadable: false,
    }];
    languages.extend(
        crate::platform::windows_speech::languages()
            .into_iter()
            .map(|(code, name)| SpeechLanguage {
                code,
                name,
                installed: true,
                downloadable: false,
            }),
    );
    Some(languages)
}

#[tauri::command]
pub fn list_local_speech_models(store: State<'_, Arc<ModelStore>>) -> Vec<LocalSpeechModel> {
    store.list()
}

#[tauri::command]
pub async fn download_local_speech_model(
    app: AppHandle,
    store: State<'_, Arc<ModelStore>>,
    model_id: String,
) -> Result<Vec<LocalSpeechModel>, CommandError> {
    store
        .download(&app, &model_id)
        .await
        .map_err(|error| CommandError::from(AppCoreError::ModelStore(error)))?;
    Ok(store.list())
}

#[tauri::command]
pub fn cancel_local_speech_model_download(store: State<'_, Arc<ModelStore>>, model_id: String) {
    store.cancel(&model_id);
}

#[tauri::command]
pub fn delete_local_speech_model(
    app: AppHandle,
    store: State<'_, Arc<ModelStore>>,
    model_id: String,
) -> Result<Vec<LocalSpeechModel>, CommandError> {
    store
        .delete(&model_id)
        .map_err(|error| CommandError::from(AppCoreError::ModelStore(error)))?;
    let models = store.list();
    let _ = app.emit("local-models-changed", &models);
    Ok(models)
}

#[tauri::command]
pub async fn detect_local_ai_servers() -> Vec<crate::ai::LocalAiServer> {
    crate::ai::detect_local_servers().await
}

#[tauri::command]
pub async fn install_local_ai_runtime(app: AppHandle) -> Result<(), CommandError> {
    crate::ai::install_runtime(&app)
        .await
        .map_err(|message| CommandError {
            code: "local_ai_install".into(),
            message,
            recoverable: true,
        })
}

#[tauri::command]
pub fn list_ai_providers() -> Vec<crate::ai::ProviderInfo> {
    crate::ai::provider_infos()
}

#[tauri::command]
pub async fn list_ai_models(
    core: State<'_, AppCore>,
    provider: Option<AiProvider>,
) -> Result<Vec<crate::ai::ListedAiModel>, CommandError> {
    // Dynamic source of truth per provider (Gemini ListModels filtered by the
    // blocklist; Zen/Go/Custom OpenAI-style listings with curated pricing),
    // so newest models appear without a Kivo update. Falls back to the
    // curated list when no key is stored or the fetch fails (offline /
    // invalid key), so the selector never appears empty. Identical on macOS
    // and Windows: keys stay in the Rust process and are sent via header.
    // The frontend passes the provider it is currently rendering so model
    // discovery cannot race an in-flight provider settings save.
    let (configured_provider, _, base_url, _) = core.ai_config();
    let provider = provider.unwrap_or(configured_provider);
    Ok(core
        .list_provider_models(provider, base_url.as_deref())
        .await)
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

/// Aggregated AI usage for the dashboard. `days` limits the window (e.g. 30);
/// omitted or `0` returns the whole local ledger. Only counts and identifiers
/// are stored, so this payload never contains prompt or response text.
#[tauri::command]
pub fn get_usage_stats(core: State<'_, AppCore>, days: Option<u32>) -> crate::usage::UsageSummary {
    let since = days
        .filter(|days| *days > 0)
        .map(|days| crate::usage::since_ms(crate::usage::now_ms(), days));
    core.usage_summary(since)
}

#[tauri::command]
pub fn clear_usage_stats(core: State<'_, AppCore>) {
    core.clear_usage_stats();
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
    Ok(selection_context(context))
}

#[tauri::command]
pub async fn run_writing_action(
    core: State<'_, AppCore>,
    request: WritingRequest,
) -> Result<WritingResponse, CommandError> {
    let action = parse_action(&request.action)?;
    let provider = core.ai_provider();
    let models_override = (!request.models.is_empty())
        .then(|| crate::ai::normalize_model_list(provider, &request.models));
    match core
        .run_writing_action_with(
            action,
            request.preset_id.as_deref(),
            request.instruction,
            request.text,
            request.source_kind,
            request.system_instruction,
            request.replaces_selection,
            models_override,
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
    // The ticket is the stored result (prevents paste-twice races), but never
    // silently substitute: if the caller's text drifted from what is stored,
    // fail instead of inserting something the user did not approve.
    let matches = core
        .last_result
        .lock()
        .map(|stored| {
            stored
                .as_ref()
                .is_some_and(|result| result.text.as_ref() == text)
        })
        .unwrap_or(false);
    if !matches {
        return Err(CommandError::from(AppCoreError::NoResult));
    }
    core.replace_with_last_result()
        .await
        .map_err(CommandError::from)?;
    Ok(())
}

#[tauri::command]
pub fn get_dictation_recovery(core: State<'_, AppCore>) -> Result<Option<String>, CommandError> {
    core.recovery_text().map_err(Into::into)
}

#[tauri::command]
pub fn clear_dictation_recovery(
    app: AppHandle,
    core: State<'_, AppCore>,
) -> Result<(), CommandError> {
    core.clear_recovery().map_err(CommandError::from)?;
    let _ = app.emit_to("settings", "recovery-changed", ());
    Ok(())
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), CommandError> {
    crate::shell::copy_text(&app, &text).map_err(platform_command_error)
}

#[tauri::command]
pub fn close_surface(
    app: AppHandle,
    surface: String,
    core: State<'_, AppCore>,
) -> Result<(), CommandError> {
    if !matches!(
        surface.as_str(),
        "flow-bar" | "writing-tools" | "settings" | "onboarding"
    ) {
        return Err(CommandError {
            code: "invalid_surface".into(),
            message: "That surface does not exist.".into(),
            recoverable: false,
        });
    }
    if surface == "writing-tools" {
        let _ = core.dismiss_writing_tools();
    }
    crate::shell::hide_surface(&app, &surface);
    if surface == "writing-tools" {
        crate::shell::sync_idle_flow_bar(&app);
    }
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
    surface: String,
    mode: String,
    height: Option<f64>,
) -> Result<(), CommandError> {
    if surface != "writing-tools" {
        return Err(CommandError {
            code: "invalid_surface".into(),
            message: "That surface cannot be resized.".into(),
            recoverable: false,
        });
    }
    if !matches!(
        mode.as_str(),
        "menu" | "custom" | "summary" | "processing" | "result" | "error"
    ) {
        return Err(CommandError {
            code: "invalid_surface".into(),
            message: "That writing-tools mode is not known.".into(),
            recoverable: false,
        });
    }
    crate::shell::set_writing_surface_focusability(&app, &mode).map_err(platform_command_error)?;
    crate::shell::size_writing_surface(&app, &mode, height).map_err(platform_command_error)?;
    Ok(())
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle) -> Result<(), CommandError> {
    crate::shell::hide_surface(&app, "onboarding");
    crate::shell::show_surface(&app, "settings", true).map_err(platform_command_error)
}

#[tauri::command]
pub fn set_paused(app: AppHandle, shell: State<'_, crate::shell::ShellState>, paused: bool) {
    shell.set_paused(&app, paused);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub(crate) current_version: String,
    pub(crate) available: bool,
    pub(crate) available_version: Option<String>,
    pub(crate) download_url: Option<String>,
    pub(crate) channel: Option<String>,
    pub(crate) current_sha: Option<String>,
    pub(crate) available_sha: Option<String>,
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

/// Whether a permission gates core functionality on `host`. Pure over the
/// host so every CI platform tests both requirement matrices: input
/// monitoring (Fn-hold detection) and OS speech recognition only exist on
/// macOS, while accessibility and microphone gate every desktop. Linux is
/// the dev/test bench: its fake speech engine needs no consent prompt, so
/// speech recognition is not required there (like Windows SAPI).
pub(crate) fn permission_required_for(
    permission: crate::platform::PermissionKind,
    host: crate::config::HostPlatform,
) -> bool {
    match permission {
        crate::platform::PermissionKind::Accessibility => true,
        crate::platform::PermissionKind::Microphone => true,
        crate::platform::PermissionKind::InputMonitoring
        | crate::platform::PermissionKind::SpeechRecognition => {
            matches!(host, crate::config::HostPlatform::Macos)
        }
    }
}

fn permission_statuses(
    platform: &crate::platform::PlatformServices,
) -> Result<Vec<FrontendPermissionStatus>, crate::platform::PlatformError> {
    let host = crate::config::HostPlatform::current();
    // Windows has no OS consent prompt for SAPI: the row reflects installed
    // engines instead, so the explanation points at the language packs.
    // Linux runs the simulated test-bench engine: always present, no prompt.
    let speech_explanation = if matches!(host, crate::config::HostPlatform::Windows) {
        "Needs an installed Windows desktop speech language."
    } else if matches!(
        host,
        crate::config::HostPlatform::Linux | crate::config::HostPlatform::Other
    ) {
        "Simulated speech engine for development and testing."
    } else {
        "Transcribe speech using the operating system."
    };
    [
        (
            crate::platform::PermissionKind::Accessibility,
            "accessibility",
            permission_required_for(crate::platform::PermissionKind::Accessibility, host),
            "Read and replace selected text.",
        ),
        (
            crate::platform::PermissionKind::InputMonitoring,
            "input-monitoring",
            permission_required_for(crate::platform::PermissionKind::InputMonitoring, host),
            "Detect the Fn hold shortcut.",
        ),
        (
            crate::platform::PermissionKind::Microphone,
            "microphone",
            permission_required_for(crate::platform::PermissionKind::Microphone, host),
            "Listen while dictation is active.",
        ),
        (
            crate::platform::PermissionKind::SpeechRecognition,
            "speech-recognition",
            permission_required_for(crate::platform::PermissionKind::SpeechRecognition, host),
            speech_explanation,
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
    WritingAction::parse_id(value).ok_or(AppCoreError::ActionDisabled)
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
    // Never parse to obtain this: a serde_json behavior change must not turn
    // settings validation into a backend panic.
    serde::de::Error::custom("invalid settings value")
}

#[cfg(test)]
mod tests;
