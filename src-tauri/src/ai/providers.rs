use std::{fmt, time::Duration};

use reqwest::{StatusCode, header::HeaderValue};
use serde::{Deserialize, Serialize};

use super::{AiPrompt, AiReasoningMode, ListedAiModel};
use crate::security::SecretString;

/// AI backends Kivo can send writing requests to.
///
/// - `Gemini` talks to Google's AI Studio endpoint directly (the original
///   Kivo behavior).
/// - `Zen` talks to the OpenCode Zen pay-as-you-go gateway
///   (`https://opencode.ai/zen/v1`, OpenAI-compatible `chat/completions`).
/// - `Go` talks to the OpenCode Go `$10/month` subscription gateway
///   (`https://opencode.ai/zen/go/v1`, same OpenAI-compatible shape).
/// - `Custom` talks to any OpenAI-compatible endpoint ("normal" OpenCode-style
///   providers: Ollama, LM Studio, llama.cpp, OpenRouter, …) via a user
///   supplied base URL.
///
/// See <https://opencode.ai/docs/providers>, <https://opencode.ai/docs/zen>
/// and <https://opencode.ai/docs/go>.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    #[default]
    Gemini,
    Zen,
    Go,
    Custom,
}

impl AiProvider {
    pub const fn all() -> &'static [Self] {
        &[Self::Gemini, Self::Zen, Self::Go, Self::Custom]
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "zen" | "opencode" | "opencode-zen" => Self::Zen,
            "go" | "opencode-go" => Self::Go,
            "custom" | "openai-compatible" | "ollama" | "local" => Self::Custom,
            _ => Self::Gemini,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Zen => "zen",
            Self::Go => "go",
            Self::Custom => "custom",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Gemini => "Gemini",
            Self::Zen => "OpenCode Zen",
            Self::Go => "OpenCode Go",
            Self::Custom => "Custom (OpenAI-compatible)",
        }
    }

    /// Secure-storage slot for this provider's API key. `gemini-api-key` is
    /// unchanged so existing installs keep working; the others are new.
    pub const fn credential_account(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-api-key",
            Self::Zen => "opencode-zen-api-key",
            Self::Go => "opencode-go-api-key",
            Self::Custom => "opencode-custom-api-key",
        }
    }

    /// Machine-readable metadata for the Settings UI, so provider labels,
    /// key links and defaults live in one place (Rust) instead of being
    /// hardcoded in the frontend.
    pub fn info(self) -> ProviderInfo {
        ProviderInfo {
            id: self.as_str().to_owned(),
            label: self.label().to_owned(),
            key_url: self.key_url().map(str::to_owned),
            key_optional: self.key_optional(),
            default_model: self.default_model().to_owned(),
            default_base_url: self.default_base_url().map(str::to_owned),
            supports_link_summary: matches!(self, Self::Gemini),
            test_uses_quota: true,
        }
    }

    /// Where to obtain an API key. `None` for local servers that need none.
    pub const fn key_url(self) -> Option<&'static str> {
        match self {
            Self::Gemini => Some("https://aistudio.google.com/app/apikey"),
            Self::Zen | Self::Go => Some("https://opencode.ai/auth"),
            Self::Custom => None,
        }
    }

    /// A key is optional only for Custom (local servers like Ollama accept
    /// requests without one); every hosted provider requires it.
    pub const fn key_optional(self) -> bool {
        matches!(self, Self::Custom)
    }

    /// Default model selected when switching to a provider whose list does
    /// not contain the current model id.
    pub const fn default_model(self) -> &'static str {
        match self {
            Self::Gemini | Self::Zen => super::DEFAULT_GEMINI_MODEL,
            // Fast, high-throughput model for short writing requests.
            Self::Go => "glm-5.3-flash",
            // Ollama's most common default; the user can type any pulled id.
            Self::Custom => "llama3.1",
        }
    }

    /// Default base URL for Custom when none is configured (Ollama's
    /// OpenAI-compatible endpoint, per <https://opencode.ai/docs/providers>).
    pub const fn default_base_url(self) -> Option<&'static str> {
        match self {
            Self::Custom => Some("http://localhost:11434/v1"),
            _ => None,
        }
    }

    pub const fn models_endpoint(self) -> Option<&'static str> {
        match self {
            Self::Gemini => None,
            Self::Zen => Some("https://opencode.ai/zen/v1/models"),
            Self::Go => Some("https://opencode.ai/zen/go/v1/models"),
            Self::Custom => None,
        }
    }

    pub const fn chat_endpoint(self) -> Option<&'static str> {
        match self {
            Self::Gemini => None,
            Self::Zen => Some("https://opencode.ai/zen/v1/chat/completions"),
            Self::Go => Some("https://opencode.ai/zen/go/v1/chat/completions"),
            Self::Custom => None,
        }
    }

    /// Resolve the models-list URL, joining a custom base URL when needed.
    pub fn resolve_models_url(self, custom_base_url: Option<&str>) -> Option<String> {
        match self {
            Self::Gemini => super::GEMINI_MODELS_ENDPOINT.to_owned().into(),
            Self::Zen | Self::Go => self.models_endpoint().map(str::to_owned),
            Self::Custom => join_base_url(
                custom_base_url
                    .filter(|base| !base.trim().is_empty())
                    .or(self.default_base_url())?,
                "models",
            ),
        }
    }

    /// Resolve the chat-completions URL, joining a custom base URL when needed.
    pub fn resolve_chat_url(self, custom_base_url: Option<&str>) -> Option<String> {
        match self {
            Self::Gemini => None,
            Self::Zen | Self::Go => self.chat_endpoint().map(str::to_owned),
            Self::Custom => join_base_url(
                custom_base_url
                    .filter(|base| !base.trim().is_empty())
                    .or(self.default_base_url())?,
                "chat/completions",
            ),
        }
    }
}

fn join_base_url(base: &str, path: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    Some(format!("{base}/{path}"))
}

/// Machine-readable provider metadata for the Settings UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
    pub key_url: Option<String>,
    pub key_optional: bool,
    pub default_model: String,
    pub default_base_url: Option<String>,
    /// Only Gemini can retrieve link content (URL context / video input).
    pub supports_link_summary: bool,
    /// Connection tests send a small generation request to validate model access.
    pub test_uses_quota: bool,
}

pub fn provider_infos() -> Vec<ProviderInfo> {
    AiProvider::all()
        .iter()
        .map(|provider| provider.info())
        .collect()
}

/// How a model is billed, shown next to the model picker so the cost of a
/// choice is visible before anything is sent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Billing {
    /// Billed per token by the upstream provider (Google for Gemini).
    #[default]
    PayPerToken,
    /// OpenCode Zen: pay-as-you-go credits at cost.
    ZenCredits,
    /// OpenCode Go: included in the `$10/month` subscription up to a monthly
    /// dollar allowance per model.
    GoSubscription,
    /// Free for a limited time (Zen/Go trial models).
    Free,
    /// Local server: no per-token charge.
    Local,
}

impl Billing {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PayPerToken => "pay_per_token",
            Self::ZenCredits => "zen_credits",
            Self::GoSubscription => "go_subscription",
            Self::Free => "free",
            Self::Local => "local",
        }
    }
}

/// Per-1M-token prices in USD plus, for Go, the included monthly allowance.
/// Curated from <https://opencode.ai/docs/zen> and
/// <https://opencode.ai/docs/go>; the live model list applies this table to
/// whatever ids the gateway currently serves, so new models appear with a
/// generic "billed per token" note until the table is updated.
mod catalog;
pub use catalog::*;

/// Errors from OpenAI-compatible providers (Zen, Go, Custom). The machine
/// readable [`OpencodeError::code`] values intentionally match the Gemini
/// ones (`invalid_api_key`, `rate_limited`, …) so the Settings UI maps them
/// to the same connection indicator without provider-specific branches.
#[derive(Debug)]
pub enum OpencodeError {
    InvalidApiKey,
    InaccessibleSource,
    Transport(reqwest::Error),
    InvalidResponse(reqwest::Error),
    Api {
        status: StatusCode,
        code: Option<String>,
    },
    EmptyResponse,
    Incomplete,
}

impl OpencodeError {
    fn provider_message(code: Option<&str>) -> Option<&str> {
        code.and_then(|code| code.strip_prefix("provider_message:"))
    }

    pub fn is_rate_limited(&self) -> bool {
        match self {
            Self::Api { status, code } => {
                *status == StatusCode::TOO_MANY_REQUESTS
                    || matches!(
                        code.as_deref(),
                        Some(
                            "rate_limited"
                                | "RESOURCE_EXHAUSTED"
                                | "rate_limit_exceeded"
                                | "quota_exceeded"
                        )
                    )
            }
            _ => false,
        }
    }

    /// The Zen balance is empty (or the Go allowance is exhausted with no
    /// balance fallback): distinct from a transient rate limit so the UI can
    /// point at billing instead of "try again shortly".
    pub fn is_out_of_credits(&self) -> bool {
        matches!(self, Self::Api { code, .. } if matches!(code.as_deref(), Some("insufficient_credits" | "insufficient_quota")))
    }

    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Api { status, code } => {
                *status == StatusCode::NOT_FOUND
                    || matches!(
                        code.as_deref(),
                        Some("not_found" | "model_not_found" | "NOT_FOUND" | "404")
                    )
            }
            _ => false,
        }
    }

    fn is_auth_code(code: Option<&str>) -> bool {
        code.is_some_and(|code| {
            [
                "authentication",
                "invalid_api_key",
                "AuthError",
                "unauthorized",
            ]
            .iter()
            .any(|known| code.eq_ignore_ascii_case(known))
        })
    }

    fn is_region_unavailable_code(code: Option<&str>) -> bool {
        code.is_some_and(|code| {
            [
                "Account.RegionUnavailable",
                "account_region_unavailable",
                "region_unavailable",
            ]
            .iter()
            .any(|known| code.eq_ignore_ascii_case(known))
        })
    }

    fn is_account_disabled_code(code: Option<&str>) -> bool {
        code.is_some_and(|code| {
            [
                "Account.Disabled",
                "account_disabled",
                "subscription_required",
            ]
            .iter()
            .any(|known| code.eq_ignore_ascii_case(known))
        })
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidApiKey => "The API key is invalid.".into(),
            Self::Incomplete => {
                "The provider stopped before completing the response. Try again or choose another model.".into()
            }
            Self::InaccessibleSource => {
                "Link summaries need the Gemini provider. Paste the text or transcript instead.".into()
            }
            Self::Api { code, .. } if Self::is_region_unavailable_code(code.as_deref()) => {
                "OpenCode Go requires Global regions for this model. Set Workspace Privacy → Regions to Global, then try again.".into()
            }
            Self::Api { code, .. } if Self::is_account_disabled_code(code.as_deref()) => {
                "OpenCode Go says this workspace does not have an active Go subscription. Check the Go subscription in the OpenCode console.".into()
            }
            Self::Api { status, code }
                if *status == StatusCode::UNAUTHORIZED || Self::is_auth_code(code.as_deref()) =>
            {
                "Couldn't connect. OpenCode did not accept this API key.".into()
            }
            _ if self.is_out_of_credits() => "The OpenCode balance is empty. Top up to continue.".into(),
            Self::Api { status, code }
                if *status == StatusCode::TOO_MANY_REQUESTS
                    || matches!(
                        code.as_deref(),
                        Some(
                            "rate_limited"
                                | "RESOURCE_EXHAUSTED"
                                | "rate_limit_exceeded"
                                | "quota_exceeded"
                        )
                    ) =>
            {
                "The provider is temporarily rate limited. Try again shortly.".into()
            }
            Self::Api { .. } if self.is_not_found() => {
                "That model isn't available. Choose another model under AI → Model.".into()
            }
            Self::Api { status, .. } if *status == StatusCode::FORBIDDEN => {
                "OpenCode rejected this request. The key may be valid, but this workspace, model, or client is not permitted to use the requested Go endpoint.".into()
            }
            Self::Api { status, code }
                if Self::provider_message(code.as_deref()).is_some() =>
            {
                let detail = Self::provider_message(code.as_deref()).unwrap_or_default();
                format!("OpenCode returned HTTP {}: {detail}", status.as_u16())
            }
            Self::Transport(_) => "Couldn't reach the AI provider. Check your connection.".into(),
            _ => "The AI provider couldn't complete that request.".into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidApiKey => "invalid_api_key",
            Self::InaccessibleSource => "inaccessible_source",
            Self::Transport(_) => "transport",
            Self::InvalidResponse(_) => "invalid_response",
            Self::Incomplete => "incomplete",
            _ if self.is_out_of_credits() => "insufficient_credits",
            Self::Api { code, .. } if Self::is_region_unavailable_code(code.as_deref()) => {
                "region_unavailable"
            }
            Self::Api { code, .. } if Self::is_account_disabled_code(code.as_deref()) => {
                "account_disabled"
            }
            Self::Api { status, code }
                if *status == StatusCode::TOO_MANY_REQUESTS
                    || matches!(
                        code.as_deref(),
                        Some(
                            "rate_limited"
                                | "RESOURCE_EXHAUSTED"
                                | "rate_limit_exceeded"
                                | "quota_exceeded"
                        )
                    ) =>
            {
                "rate_limited"
            }
            Self::Api { status, code }
                if *status == StatusCode::UNAUTHORIZED || Self::is_auth_code(code.as_deref()) =>
            {
                "invalid_api_key"
            }
            Self::Api { .. } if self.is_not_found() => "model_not_found",
            Self::Api { status, .. } if *status == StatusCode::FORBIDDEN => "provider_forbidden",
            Self::Api { status, code }
                if *status == StatusCode::BAD_REQUEST
                    && Self::provider_message(code.as_deref()).is_some() =>
            {
                "provider_rejected"
            }
            Self::Api { .. } => "api_error",
            Self::EmptyResponse => "empty_response",
        }
    }
}

impl fmt::Display for OpencodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.user_message())
    }
}

impl std::error::Error for OpencodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) | Self::InvalidResponse(error) => Some(error),
            _ => None,
        }
    }
}

/// Failures shared by every queue entry: the key/account is unusable, the
/// balance is empty, or the host is unreachable, so trying the next model cannot help
/// (and must not burn quota).
fn is_failover_terminal(error: &OpencodeError) -> bool {
    match error {
        OpencodeError::InvalidApiKey => true,
        OpencodeError::Transport(error) => !error.is_timeout(),
        OpencodeError::Api { status, code }
            if *status == StatusCode::UNAUTHORIZED
                || OpencodeError::is_auth_code(code.as_deref())
                || OpencodeError::is_account_disabled_code(code.as_deref()) =>
        {
            true
        }
        _ => error.is_out_of_credits(),
    }
}

#[derive(Clone, Copy)]
struct CompatRequestContext<'a> {
    chat_url: &'a str,
    api_key: Option<&'a SecretString>,
    session: Option<&'a str>,
}

#[derive(Clone)]
pub struct OpenAiCompatClient {
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    pub fn new() -> Result<Self, OpencodeError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(OpencodeError::Transport)?;
        Ok(Self { http })
    }

    fn next_opencode_session(provider: AiProvider) -> Option<String> {
        if !matches!(provider, AiProvider::Zen | AiProvider::Go) {
            return None;
        }
        static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(format!(
            "kivo-{}-{}-{sequence}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn chat_request(
        &self,
        provider: AiProvider,
        chat_url: &str,
        session: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let request = self.http.post(chat_url);
        if matches!(provider, AiProvider::Zen | AiProvider::Go) {
            let session = session.expect("OpenCode requests require a session id");
            request
                .header("user-agent", concat!("Kivo/", env!("CARGO_PKG_VERSION")))
                .header("x-opencode-client", "kivo")
                .header("x-opencode-session", session)
        } else {
            request
        }
    }

    fn auth_header(api_key: Option<&SecretString>) -> Result<Option<HeaderValue>, OpencodeError> {
        api_key
            .map(|key| HeaderValue::from_str(&format!("Bearer {}", key.expose())))
            .transpose()
            .map_err(|_| OpencodeError::InvalidApiKey)
    }

    fn resolve_model(provider: AiProvider, model: &str) -> String {
        normalize_model_for(provider, model)
    }

    /// Dynamic model picker source: `GET {models_url}` in OpenAI list shape
    /// (`{"data":[{"id":…}]}`), priced from the curated tables. The Zen/Go
    /// listings are public (no key needed); custom servers may or may not
    /// require one. Falls back to the curated list on failure so the selector
    /// never appears empty offline.
    pub async fn list_models(
        &self,
        provider: AiProvider,
        models_url: &str,
        api_key: Option<&SecretString>,
    ) -> Result<Vec<ListedAiModel>, OpencodeError> {
        let mut request = self.http.get(models_url);
        if matches!(provider, AiProvider::Zen | AiProvider::Go) {
            request = request.header("user-agent", concat!("Kivo/", env!("CARGO_PKG_VERSION")));
        }
        if let Some(auth) = Self::auth_header(api_key)? {
            request = request.header("authorization", auth);
        }
        let response = request.send().await.map_err(OpencodeError::Transport)?;
        if !response.status().is_success() {
            return Err(parse_opencode_api_error(response).await);
        }
        let page = response
            .json::<OpenAiModelList>()
            .await
            .map_err(OpencodeError::InvalidResponse)?;
        let mut listed: Vec<ListedAiModel> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in page.data {
            let Some(model) = listed_opencode_model(provider, &entry.id) else {
                continue;
            };
            if seen.insert(model.id.clone()) {
                listed.push(model);
            }
        }
        if listed.is_empty() {
            listed = curated_models_for(provider);
        } else {
            sort_listed_models(&mut listed, provider);
        }
        Ok(listed)
    }

    async fn generate_one(
        &self,
        provider: AiProvider,
        context: CompatRequestContext<'_>,
        model: &str,
        prompt: &AiPrompt,
        reasoning_mode: AiReasoningMode,
    ) -> Result<(String, Option<u64>, Option<u64>), OpencodeError> {
        let model = Self::resolve_model(provider, model);
        let request = ChatCompletionRequest {
            model: model.as_str(),
            messages: &[
                ChatMessage {
                    role: "system",
                    content: &prompt.system_instruction,
                },
                ChatMessage {
                    role: "user",
                    content: &prompt.input,
                },
            ],
            max_tokens: None,
        };
        let protocol = completion_protocol(provider, &model);
        let (url, body) = completion_request(
            protocol,
            provider,
            reasoning_mode,
            context.chat_url,
            &request,
        );
        let mut call = self
            .chat_request(provider, &url, context.session)
            .json(&body);
        if protocol == CompletionProtocol::Messages {
            call = call.header("anthropic-version", "2023-06-01");
        }
        if let Some(auth) = Self::auth_header(context.api_key)? {
            call = call.header("authorization", auth);
        }
        if protocol == CompletionProtocol::Messages
            && let Some(key) = context.api_key
        {
            let header =
                HeaderValue::from_str(key.expose()).map_err(|_| OpencodeError::InvalidApiKey)?;
            call = call.header("x-api-key", header);
        } else if protocol == CompletionProtocol::Google
            && let Some(key) = context.api_key
        {
            // OpenCode's Zen Google adapter follows the Google SDK contract
            // and expects the key in x-goog-api-key, unlike its OpenAI and
            // Anthropic adapters.
            let header =
                HeaderValue::from_str(key.expose()).map_err(|_| OpencodeError::InvalidApiKey)?;
            call = call.header("x-goog-api-key", header);
        }
        let response = call.send().await.map_err(OpencodeError::Transport)?;
        if !response.status().is_success() {
            return Err(parse_opencode_api_error(response).await);
        }
        let completion = response
            .json::<serde_json::Value>()
            .await
            .map_err(OpencodeError::InvalidResponse)?;
        parse_completion_with_usage(protocol, completion)
    }

    /// Authenticated generation probe: public model listings cannot validate a key.
    pub async fn test_connection(
        &self,
        provider: AiProvider,
        chat_url: &str,
        api_key: Option<&SecretString>,
        model: &str,
    ) -> Result<(), OpencodeError> {
        if !provider.key_optional() && api_key.is_none() {
            return Err(OpencodeError::InvalidApiKey);
        }
        let model = Self::resolve_model(provider, model);
        let request = ChatCompletionRequest {
            model: model.as_str(),
            messages: &[ChatMessage {
                role: "user",
                content: "ok",
            }],
            // A few OpenAI-compatible gateways reject ultra-small output limits.
            // Sixteen tokens is still effectively free for a connection probe and
            // matches the minimum Kivo already uses for Responses requests.
            max_tokens: Some(16),
        };
        let protocol = completion_protocol(provider, &model);
        let (url, mut body) = completion_request(
            protocol,
            provider,
            AiReasoningMode::Fast,
            chat_url,
            &request,
        );
        // A connection probe should test auth + routing with the smallest
        // provider-compatible request, not Kivo's optional generation tuning.
        // Leaving reasoning controls out also makes failures easier to attribute.
        if let Some(object) = body.as_object_mut() {
            object.remove("reasoning_effort");
            object.remove("thinking");
            object.remove("output_config");
        }
        let session = Self::next_opencode_session(provider);
        let mut call = self
            .chat_request(provider, &url, session.as_deref())
            .json(&body);
        if protocol == CompletionProtocol::Messages {
            call = call.header("anthropic-version", "2023-06-01");
        }
        if let Some(auth) = Self::auth_header(api_key)? {
            call = call.header("authorization", auth);
        }
        if protocol == CompletionProtocol::Messages
            && let Some(key) = api_key
        {
            let header =
                HeaderValue::from_str(key.expose()).map_err(|_| OpencodeError::InvalidApiKey)?;
            call = call.header("x-api-key", header);
        } else if protocol == CompletionProtocol::Google
            && let Some(key) = api_key
        {
            let header =
                HeaderValue::from_str(key.expose()).map_err(|_| OpencodeError::InvalidApiKey)?;
            call = call.header("x-goog-api-key", header);
        }
        let response = call.send().await.map_err(OpencodeError::Transport)?;
        if !response.status().is_success() {
            return Err(parse_opencode_api_error(response).await);
        }
        Ok(())
    }

    /// Try each model in order until one succeeds. Key/account failures
    /// (auth, empty balance) abort immediately: every entry shares the same
    /// key, so continuing could only burn quota. Anything else — rate
    /// limits, unknown model ids, server errors, empty responses — falls
    /// through to the next model, and the last error is returned when all
    /// fail.
    pub async fn generate_in_order_with_model(
        &self,
        provider: AiProvider,
        chat_url: &str,
        api_key: Option<&SecretString>,
        models: &[String],
        prompt: &AiPrompt,
        reasoning_mode: AiReasoningMode,
    ) -> Result<(String, String, Option<u64>, Option<u64>), OpencodeError> {
        let normalized = normalize_model_list_for(provider, models);
        let session = Self::next_opencode_session(provider);
        let context = CompatRequestContext {
            chat_url,
            api_key,
            session: session.as_deref(),
        };
        let mut last_error: Option<OpencodeError> = None;
        for model in &normalized {
            match self
                .generate_one(provider, context, model, prompt, reasoning_mode)
                .await
            {
                Ok((text, tokens_in, tokens_out)) => {
                    return Ok((text, model.clone(), tokens_in, tokens_out));
                }
                Err(error) if is_failover_terminal(&error) => return Err(error),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or(OpencodeError::EmptyResponse))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CompletionProtocol {
    Chat,
    Messages,
    Responses,
    Google,
}

fn completion_protocol(provider: AiProvider, model: &str) -> CompletionProtocol {
    // Zen and Go publish model-specific protocols. Custom endpoints retain
    // their explicitly configured OpenAI chat-completions contract.
    if matches!(provider, AiProvider::Zen | AiProvider::Go) {
        if provider == AiProvider::Zen && model.starts_with("gemini-") {
            return CompletionProtocol::Google;
        }
        if model.starts_with("claude-")
            || model.starts_with("qwen")
            || (provider == AiProvider::Go
                && (model.starts_with("minimax-") || model == "union-alpha"))
        {
            return CompletionProtocol::Messages;
        }
        if model.starts_with("gpt-") || model.starts_with("grok-") || model.starts_with("muse-") {
            return CompletionProtocol::Responses;
        }
    }
    CompletionProtocol::Chat
}

fn chat_reasoning_effort(
    provider: AiProvider,
    model: &str,
    reasoning_mode: AiReasoningMode,
) -> Option<&'static str> {
    if !matches!(provider, AiProvider::Zen | AiProvider::Go) {
        return None;
    }
    let model = model.to_ascii_lowercase();
    match model.as_str() {
        // models.dev documents all three effort values for these exact ids.
        "glm-5.3-flash"
        | "glm-5.3"
        | "deepseek-v4.1-flash"
        | "deepseek-v4-flash"
        | "deepseek-v4-flash-vision-exp" => Some(match reasoning_mode {
            AiReasoningMode::Fast => "low",
            AiReasoningMode::Balanced => "high",
            AiReasoningMode::Deep => "max",
        }),
        // These models expose high/max only. Fast leaves the provider default
        // in place instead of sending an unsupported low value.
        "glm-5.2" | "deepseek-v4-pro" => match reasoning_mode {
            AiReasoningMode::Fast => None,
            AiReasoningMode::Balanced => Some("high"),
            AiReasoningMode::Deep => Some("max"),
        },
        // Toggle-only and unknown models intentionally receive no guessed
        // reasoning_effort field.
        _ => None,
    }
}

fn messages_reasoning(
    provider: AiProvider,
    model: &str,
    reasoning_mode: AiReasoningMode,
) -> Option<(&'static str, Option<&'static str>)> {
    if !matches!(provider, AiProvider::Zen | AiProvider::Go)
        || !model.eq_ignore_ascii_case("qwen3.8-flash")
    {
        return None;
    }
    Some(match reasoning_mode {
        AiReasoningMode::Fast => ("disabled", None),
        AiReasoningMode::Balanced => ("enabled", Some("medium")),
        AiReasoningMode::Deep => ("enabled", Some("xhigh")),
    })
}

fn completion_request(
    protocol: CompletionProtocol,
    provider: AiProvider,
    reasoning_mode: AiReasoningMode,
    chat_url: &str,
    request: &ChatCompletionRequest<'_>,
) -> (String, serde_json::Value) {
    let base = chat_url
        .strip_suffix("/chat/completions")
        .unwrap_or(chat_url);
    match protocol {
        CompletionProtocol::Chat => {
            let mut body = serde_json::to_value(request).expect("serializable request");
            if let Some(effort) = chat_reasoning_effort(provider, request.model, reasoning_mode) {
                body["reasoning_effort"] = effort.into();
            }
            (chat_url.to_owned(), body)
        }
        CompletionProtocol::Messages => {
            let system = request
                .messages
                .iter()
                .filter(|message| message.role == "system")
                .map(|message| message.content)
                .collect::<Vec<_>>()
                .join("\n");
            let messages: Vec<_> = request
                .messages
                .iter()
                .filter(|message| message.role != "system")
                .collect();
            let mut body = serde_json::json!({
                "model": request.model,
                "system": system,
                "messages": messages,
                // Anthropic's protocol requires an explicit output limit.
                "max_tokens": request.max_tokens.unwrap_or(8192),
            });
            if let Some((thinking_type, effort)) =
                messages_reasoning(provider, request.model, reasoning_mode)
            {
                body["thinking"] = serde_json::json!({"type": thinking_type});
                if let Some(effort) = effort {
                    body["output_config"] = serde_json::json!({"effort": effort});
                }
            }
            (format!("{base}/messages"), body)
        }
        CompletionProtocol::Responses => {
            let mut body = serde_json::json!({
                "model": request.model,
                "input": request.messages,
                "store": false,
            });
            if let Some(limit) = request.max_tokens {
                // The Responses protocol requires at least 16 output tokens.
                body["max_output_tokens"] = limit.max(16).into();
            }
            (format!("{base}/responses"), body)
        }
        CompletionProtocol::Google => {
            let system = request
                .messages
                .iter()
                .filter(|message| message.role == "system")
                .map(|message| serde_json::json!({"text": message.content}))
                .collect::<Vec<_>>();
            let contents = request
                .messages
                .iter()
                .filter(|message| message.role != "system")
                .map(|message| {
                    serde_json::json!({
                        "role": "user",
                        "parts": [{"text": message.content}],
                    })
                })
                .collect::<Vec<_>>();
            let mut body = serde_json::json!({
                "contents": contents,
            });
            if !system.is_empty() {
                body["systemInstruction"] = serde_json::json!({"parts": system});
            }
            if let Some(limit) = request.max_tokens {
                body["generationConfig"] = serde_json::json!({"maxOutputTokens": limit});
            }
            (
                format!("{base}/models/{}:generateContent", request.model),
                body,
            )
        }
    }
}

#[cfg(test)]
fn parse_completion(
    protocol: CompletionProtocol,
    body: serde_json::Value,
) -> Result<String, OpencodeError> {
    parse_completion_with_usage(protocol, body).map(|(text, _, _)| text)
}

fn parse_completion_with_usage(
    protocol: CompletionProtocol,
    body: serde_json::Value,
) -> Result<(String, Option<u64>, Option<u64>), OpencodeError> {
    let usage = body.get("usage").or_else(|| body.get("usageMetadata"));
    let token = |names: &[&str]| {
        usage.and_then(|value| {
            names
                .iter()
                .find_map(|name| value.get(name).and_then(serde_json::Value::as_u64))
        })
    };
    let (tokens_in, tokens_out) = match protocol {
        CompletionProtocol::Messages => (
            token(&["input_tokens", "prompt_tokens"]),
            token(&["output_tokens", "completion_tokens"]),
        ),
        CompletionProtocol::Google => (
            token(&["promptTokenCount", "prompt_tokens"]),
            token(&["candidatesTokenCount", "completion_tokens"]),
        ),
        CompletionProtocol::Chat | CompletionProtocol::Responses => (
            token(&["prompt_tokens", "input_tokens"]),
            token(&["completion_tokens", "output_tokens"]),
        ),
    };
    let incomplete = match protocol {
        CompletionProtocol::Messages => matches!(
            body["stop_reason"].as_str(),
            Some("max_tokens" | "pause_turn")
        ),
        CompletionProtocol::Responses => matches!(
            body["status"].as_str(),
            Some("incomplete" | "failed" | "cancelled" | "in_progress" | "queued")
        ),
        CompletionProtocol::Chat => body["choices"].as_array().is_some_and(|choices| {
            choices
                .iter()
                .any(|choice| choice["finish_reason"] == "length")
        }),
        CompletionProtocol::Google => body["candidates"].as_array().is_some_and(|candidates| {
            candidates
                .iter()
                .any(|candidate| matches!(candidate["finishReason"].as_str(), Some("MAX_TOKENS")))
        }),
    };
    if incomplete {
        return Err(OpencodeError::Incomplete);
    }
    if protocol == CompletionProtocol::Chat {
        // Malformed successful payloads must not count as completed writing.
        let response = serde_json::from_value(body).map_err(|_| OpencodeError::EmptyResponse)?;
        return parse_chat_completion(response).map(|text| (text, tokens_in, tokens_out));
    }
    let parts: Vec<&serde_json::Value> = match protocol {
        CompletionProtocol::Messages => body
            .get("content")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part["type"] == "text")
            .collect(),
        CompletionProtocol::Responses => body
            .get("output")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "message")
            .flat_map(|item| {
                item.get("content")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|part| part["type"] == "output_text")
            .collect(),
        CompletionProtocol::Google => body
            .get("candidates")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|candidate| {
                candidate
                    .get("content")
                    .and_then(|content| content.get("parts"))
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|part| part.get("text").is_some())
            .collect(),
        CompletionProtocol::Chat => unreachable!(),
    };
    let output = parts
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<String>();
    let output = output.trim();
    if output.is_empty() {
        Err(OpencodeError::EmptyResponse)
    } else {
        Ok((output.to_owned(), tokens_in, tokens_out))
    }
}

#[derive(Serialize)]
// Omit temperature: models such as Kimi only accept their default sampling settings.
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage<'a>],
    // No output cap on writing requests: prompt budgets were dropped alongside
    // the Gemini `max_output_tokens` (same reasoning — small sources produce
    // small outputs). Only the connection probe uses a tiny output cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelList {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    #[serde(default)]
    id: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatResponseMessage>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: Option<ChatContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Deserialize)]
struct ChatContentPart {
    #[serde(default, rename = "type")]
    part_type: String,
    #[serde(default)]
    text: Option<String>,
}

async fn parse_opencode_api_error(response: reqwest::Response) -> OpencodeError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let code = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|body| parse_openai_error_code(&body))
        .or_else(|| provider_message_code(&text));
    OpencodeError::Api { status, code }
}

fn provider_message_code(message: &str) -> Option<String> {
    let detail: String = message
        .trim()
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(500)
        .collect();
    (!detail.is_empty()).then(|| format!("provider_message:{detail}"))
}

/// Normalize the OpenAI-style error payload into a short machine-readable
/// code. Gateways answer `{"error": {"message": …, "type": "AuthError"}}`
/// (Zen/Go use `AuthError` for bad keys) or the OpenAI
/// `{"error": {"code": "model_not_found"}}` shape.
fn parse_openai_error_code(body: &serde_json::Value) -> Option<String> {
    let error = body.get("error").unwrap_or(body);
    let classify_message = |message: &str| {
        let lower = message.to_lowercase();
        if lower.contains("global region") || lower.contains("global regions") {
            Some("Account.RegionUnavailable")
        } else if lower.contains("active opencode go subscription")
            || lower.contains("go subscription is required")
            || lower.contains("subscribe to go")
        {
            Some("Account.Disabled")
        } else if lower.contains("invalid api key")
            || lower.contains("incorrect api key")
            || lower.contains("unauthorized")
        {
            Some("authentication")
        } else {
            None
        }
    };
    if let Some(text) = error.as_str() {
        return classify_message(text)
            .map(str::to_owned)
            .or_else(|| provider_message_code(text));
    }
    let kind = error.get("type").and_then(serde_json::Value::as_str);
    let code = error.get("code").and_then(|code| match code {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    });
    let message = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let message_lower = message.to_lowercase();

    if let Some(classified) = classify_message(message) {
        return Some(classified.into());
    }
    if matches!(kind, Some("AuthError")) {
        return Some("authentication".into());
    }
    if message_lower.contains("insufficient")
        && (message_lower.contains("balance")
            || message_lower.contains("quota")
            || message_lower.contains("credit"))
    {
        return Some("insufficient_credits".into());
    }
    if matches!(
        code.as_deref(),
        Some("model_not_found" | "model_not_supported" | "invalid_model" | "model_not_allowed")
    ) || message_lower.contains("model not found")
        || message_lower.contains("no such model")
        || message_lower.contains("does not exist")
    {
        return Some("not_found".into());
    }
    if matches!(
        code.as_deref(),
        Some("rate_limit_exceeded" | "rate_limited" | "quota_exceeded")
    ) || message_lower.contains("rate limit")
        || message_lower.contains("rate_limit")
    {
        return Some("rate_limited".into());
    }
    if !message.is_empty() {
        return provider_message_code(message);
    }
    code.or_else(|| kind.map(str::to_owned))
}

fn parse_chat_completion(response: ChatCompletionResponse) -> Result<String, OpencodeError> {
    let output = response
        .choices
        .iter()
        .filter_map(|choice| choice.message.as_ref())
        .filter_map(|message| message.content.as_ref())
        .map(|content| match content {
            ChatContent::Text(text) => text.clone(),
            ChatContent::Parts(parts) => parts
                .iter()
                .filter(|part| part.part_type == "text")
                .filter_map(|part| part.text.as_deref())
                .collect::<String>(),
        })
        .collect::<String>();
    let output = output.trim();
    if output.is_empty() {
        return Err(OpencodeError::EmptyResponse);
    }
    Ok(output.to_owned())
}

#[cfg(test)]
mod tests;
