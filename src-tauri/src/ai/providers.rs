use std::{fmt, time::Duration};

use reqwest::{StatusCode, header::HeaderValue};
use serde::{Deserialize, Serialize};

use super::{AiPrompt, ListedAiModel};
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPrice {
    pub input_per_1m: f64,
    pub output_per_1m: f64,
    pub cached_per_1m: f64,
    /// Go only: monthly dollar allowance included in the subscription.
    pub monthly_limit_usd: Option<f64>,
}

impl ModelPrice {
    const fn payg(input: f64, output: f64, cached: f64) -> Self {
        Self {
            input_per_1m: input,
            output_per_1m: output,
            cached_per_1m: cached,
            monthly_limit_usd: None,
        }
    }

    const fn go(input: f64, output: f64, cached: f64, monthly: f64) -> Self {
        Self {
            input_per_1m: input,
            output_per_1m: output,
            cached_per_1m: cached,
            monthly_limit_usd: Some(monthly),
        }
    }
}

/// (model id, price) for the Zen pay-as-you-go gateway.
const ZEN_PRICES: &[(&str, ModelPrice)] = &[
    ("gemini-3.8-flash", ModelPrice::payg(1.50, 7.50, 0.15)),
    ("gemini-3.7-flash", ModelPrice::payg(1.50, 7.50, 0.15)),
    ("gemini-3.6-flash", ModelPrice::payg(1.50, 7.50, 0.15)),
    ("gemini-3.5-flash", ModelPrice::payg(1.50, 9.00, 0.15)),
    ("gemini-3.5-flash-lite", ModelPrice::payg(0.30, 2.50, 0.03)),
    ("gemini-3.1-pro", ModelPrice::payg(2.00, 12.00, 0.20)),
    ("gemini-3-flash", ModelPrice::payg(0.50, 3.00, 0.05)),
    ("gpt-6-astra", ModelPrice::payg(10.00, 50.00, 1.00)),
    ("gpt-5.6-sol", ModelPrice::payg(2.00, 10.00, 0.20)),
    ("gpt-5.6-terra", ModelPrice::payg(2.00, 12.00, 0.20)),
    ("gpt-5.6-luna", ModelPrice::payg(0.20, 1.20, 0.02)),
    ("gpt-5.5", ModelPrice::payg(5.00, 30.00, 0.50)),
    ("gpt-5.5-pro", ModelPrice::payg(30.00, 180.00, 30.00)),
    ("gpt-5.4", ModelPrice::payg(2.50, 15.00, 0.25)),
    ("gpt-5.4-pro", ModelPrice::payg(30.00, 180.00, 30.00)),
    ("gpt-5.4-mini", ModelPrice::payg(0.75, 4.50, 0.075)),
    ("gpt-5.4-nano", ModelPrice::payg(0.20, 1.25, 0.02)),
    ("gpt-5.3-codex", ModelPrice::payg(1.75, 14.00, 0.175)),
    ("gpt-5.3-codex-spark", ModelPrice::payg(1.75, 14.00, 0.175)),
    ("gpt-5.2", ModelPrice::payg(1.75, 14.00, 0.175)),
    ("gpt-5.2-codex", ModelPrice::payg(1.75, 14.00, 0.175)),
    ("gpt-5.1", ModelPrice::payg(1.07, 8.50, 0.107)),
    ("gpt-5.1-codex", ModelPrice::payg(1.07, 8.50, 0.107)),
    ("gpt-5.1-codex-max", ModelPrice::payg(1.25, 10.00, 0.125)),
    ("gpt-5.1-codex-mini", ModelPrice::payg(0.25, 2.00, 0.025)),
    ("gpt-5", ModelPrice::payg(1.07, 8.50, 0.107)),
    ("gpt-5-codex", ModelPrice::payg(1.07, 8.50, 0.107)),
    ("gpt-5-nano", ModelPrice::payg(0.05, 0.40, 0.005)),
    ("claude-fable-5-1", ModelPrice::payg(10.00, 50.00, 0.25)),
    ("claude-fable-5", ModelPrice::payg(10.00, 50.00, 1.00)),
    ("claude-opus-5", ModelPrice::payg(5.00, 25.00, 0.50)),
    ("claude-opus-4-8", ModelPrice::payg(5.00, 25.00, 0.50)),
    ("claude-opus-4-7", ModelPrice::payg(5.00, 25.00, 0.50)),
    ("claude-opus-4-6", ModelPrice::payg(5.00, 25.00, 0.50)),
    ("claude-opus-4-5", ModelPrice::payg(5.00, 25.00, 0.50)),
    ("claude-sonnet-5", ModelPrice::payg(2.00, 10.00, 0.20)),
    ("claude-sonnet-4-6", ModelPrice::payg(3.00, 15.00, 0.30)),
    ("claude-sonnet-4-5", ModelPrice::payg(3.00, 15.00, 0.30)),
    ("claude-haiku-4-5", ModelPrice::payg(1.00, 5.00, 0.10)),
    ("grok-4.6", ModelPrice::payg(2.00, 6.00, 0.50)),
    ("grok-4.5", ModelPrice::payg(2.00, 6.00, 0.30)),
    ("grok-build-0.1", ModelPrice::payg(1.00, 2.00, 0.20)),
    ("muse-spark-1.3", ModelPrice::payg(1.25, 4.25, 0.15)),
    ("muse-spark-1.2", ModelPrice::payg(1.25, 4.25, 0.15)),
    ("qwen3.7-max", ModelPrice::payg(2.50, 7.50, 0.50)),
    ("qwen3.7-plus", ModelPrice::payg(0.40, 1.60, 0.04)),
    ("qwen3.6-plus", ModelPrice::payg(0.50, 3.00, 0.05)),
    ("qwen3.5-plus", ModelPrice::payg(0.20, 1.20, 0.02)),
    ("deepseek-v4-pro", ModelPrice::payg(1.74, 3.48, 0.145)),
    ("deepseek-v4-flash", ModelPrice::payg(0.14, 0.28, 0.028)),
    (
        "deepseek-v4-flash-vision-exp",
        ModelPrice::payg(0.14, 0.28, 0.028),
    ),
    ("minimax-m3", ModelPrice::payg(0.30, 1.20, 0.06)),
    ("minimax-m2.7", ModelPrice::payg(0.30, 1.20, 0.06)),
    ("minimax-m2.5", ModelPrice::payg(0.30, 1.20, 0.06)),
    ("glm-5.3-flash", ModelPrice::payg(0.15, 0.50, 0.03)),
    ("glm-5.3", ModelPrice::payg(1.40, 4.40, 0.26)),
    ("glm-5.2", ModelPrice::payg(1.40, 4.40, 0.26)),
    ("glm-5.1", ModelPrice::payg(1.40, 4.40, 0.26)),
    ("glm-5", ModelPrice::payg(1.00, 3.20, 0.20)),
    ("kimi-k3", ModelPrice::payg(3.00, 15.00, 0.30)),
    ("kimi-k2.7-code", ModelPrice::payg(0.95, 4.00, 0.19)),
    ("kimi-k2.6", ModelPrice::payg(0.95, 4.00, 0.16)),
    ("kimi-k2.5", ModelPrice::payg(0.60, 3.00, 0.10)),
];

/// Free Zen trial models (limited time, zero-retention exceptions documented
/// at <https://opencode.ai/docs/zen#privacy>).
const ZEN_FREE_MODELS: &[&str] = &[
    "big-pickle",
    "union-alpha",
    "mimo-v2.5-free",
    "ling-3.0-flash-fin-free",
    "nemotron-3-ultra-free",
    "nemotron-3.5-lightning-free",
    "muse-spark-1.3-contributor-free",
];

/// (model id, price + included monthly allowance) for the Go subscription.
const GO_PRICES: &[(&str, ModelPrice)] = &[
    ("glm-5.3-flash", ModelPrice::go(0.15, 0.50, 0.03, 60.0)),
    ("glm-5.3", ModelPrice::go(1.40, 4.40, 0.26, 15.0)),
    ("glm-5.2", ModelPrice::go(1.40, 4.40, 0.26, 60.0)),
    ("glm-5.1", ModelPrice::go(1.40, 4.40, 0.26, 60.0)),
    ("kimi-k3", ModelPrice::go(3.00, 15.00, 0.30, 15.0)),
    ("kimi-k2.7-code", ModelPrice::go(0.95, 4.00, 0.19, 60.0)),
    ("kimi-k2.6", ModelPrice::go(0.95, 4.00, 0.16, 60.0)),
    ("longcat-2.0", ModelPrice::go(0.30, 1.20, 0.006, 60.0)),
    ("mimo-v2.5", ModelPrice::go(0.14, 0.28, 0.0028, 60.0)),
    ("mimo-v2.5-pro", ModelPrice::go(0.435, 0.87, 0.003625, 15.0)),
    ("minimax-m3", ModelPrice::go(0.30, 1.20, 0.06, 60.0)),
    ("minimax-m2.7", ModelPrice::go(0.30, 1.20, 0.06, 60.0)),
    ("minimax-m2.5", ModelPrice::go(0.30, 1.20, 0.06, 60.0)),
    (
        "muse-spark-1.3-contributor",
        ModelPrice::go(0.10, 0.20, 0.002, 60.0),
    ),
    (
        "muse-spark-1.2-contributor",
        ModelPrice::go(0.10, 0.20, 0.002, 60.0),
    ),
    ("qwen3.8-max", ModelPrice::go(2.00, 6.00, 0.25, 15.0)),
    ("qwen3.8-flash", ModelPrice::go(0.15, 0.47, 0.016, 30.0)),
    ("qwen3.7-max", ModelPrice::go(2.50, 7.50, 0.50, 30.0)),
    ("qwen3.7-plus", ModelPrice::go(0.40, 1.60, 0.04, 60.0)),
    ("qwen3.6-plus", ModelPrice::go(0.50, 3.00, 0.05, 60.0)),
    (
        "deepseek-v4.1-flash",
        ModelPrice::go(0.15, 0.60, 0.003, 60.0),
    ),
    ("deepseek-v4-pro", ModelPrice::go(0.66, 1.98, 0.022, 15.0)),
    ("deepseek-v4-flash", ModelPrice::go(0.15, 0.60, 0.003, 30.0)),
    (
        "deepseek-v4-flash-vision-exp",
        ModelPrice::go(0.15, 0.60, 0.003, 15.0),
    ),
    ("hy4-preview", ModelPrice::go(0.834, 2.501, 0.042, 30.0)),
    ("hy3", ModelPrice::go(0.14, 0.58, 0.035, 60.0)),
    ("grok-4.6", ModelPrice::go(2.00, 6.00, 0.50, 15.0)),
    ("gpt-5.6-luna", ModelPrice::go(0.20, 1.20, 0.02, 15.0)),
];

/// Free Go models (limited time).
const GO_FREE_MODELS: &[&str] = &["union-alpha"];

pub fn price_for(provider: AiProvider, id: &str) -> Option<ModelPrice> {
    let table = match provider {
        AiProvider::Zen => ZEN_PRICES,
        AiProvider::Go => GO_PRICES,
        _ => return None,
    };
    table
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, price)| *price)
}

pub fn billing_for(provider: AiProvider, id: &str) -> Billing {
    match provider {
        AiProvider::Gemini => Billing::PayPerToken,
        AiProvider::Zen if ZEN_FREE_MODELS.contains(&id) => Billing::Free,
        AiProvider::Zen => Billing::ZenCredits,
        AiProvider::Go if GO_FREE_MODELS.contains(&id) => Billing::Free,
        AiProvider::Go => Billing::GoSubscription,
        AiProvider::Custom => Billing::Local,
    }
}

fn fmt_usd(value: f64) -> String {
    if value >= 1.0 {
        format!("${value:.2}")
    } else if value >= 0.01 {
        let rounded = (value * 100.0).round() / 100.0;
        format!("${rounded:.2}")
    } else {
        let mut text = format!("${value:.4}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.push('0');
        }
        text
    }
}

fn fmt_monthly(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("${value:.0}")
    } else {
        format!("${value:.2}")
    }
}

/// Short cost summary for the model picker, e.g.
/// `$0.95 in / $4.00 out per 1M · $60/mo incl.` or `Free`.
pub fn cost_label_for(provider: AiProvider, id: &str) -> Option<String> {
    match billing_for(provider, id) {
        Billing::Free => Some("Free".into()),
        Billing::Local => Some("Local · free".into()),
        Billing::PayPerToken => None,
        Billing::ZenCredits | Billing::GoSubscription => {
            let price = price_for(provider, id)?;
            let mut label = format!(
                "{} in / {} out per 1M",
                fmt_usd(price.input_per_1m),
                fmt_usd(price.output_per_1m)
            );
            if let Some(monthly) = price.monthly_limit_usd {
                label.push_str(&format!(" · {}/mo incl.", fmt_monthly(monthly)));
            }
            Some(label)
        }
    }
}

/// Canonicalize an OpenCode-style model id: trim whitespace and strip the
/// `opencode/` / `opencode-go/` provider prefix used in `opencode.json`
/// (`opencode/gpt-5.5` → `gpt-5.5`).
pub fn canonical_opencode_id(id: &str) -> String {
    let trimmed = id.trim();
    trimmed
        .strip_prefix("opencode-go/")
        .or_else(|| trimmed.strip_prefix("opencode/"))
        .unwrap_or(trimmed)
        .to_owned()
}

/// Zen/Go model ids are lowercase with dots, digits, dashes and underscores
/// (`kimi-k2.7-code`, `qwen3.7-plus`). Non-text families are rejected with the
/// same blocklist as Gemini, except `omni`: Go serves a text-capable
/// `mimo-v2-omni` that would otherwise collide with the video-family rule.
pub fn is_usable_opencode_model(id: &str) -> bool {
    let canonical = canonical_opencode_id(id);
    if canonical.len() < 3 || canonical.len() > 128 {
        return false;
    }
    if !canonical.contains('-') && !canonical.contains('.') {
        return false;
    }
    if !canonical
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '_')
    {
        return false;
    }
    let lower = canonical.to_lowercase();
    super::BLOCKED_MODEL_SUBSTRINGS
        .iter()
        .filter(|blocked| **blocked != "omni")
        .all(|blocked| !lower.contains(*blocked))
}

/// Custom (OpenAI-compatible) model ids mirror what local servers accept:
/// Ollama (`llama3.1`, `google/gemma-3n-e4b`), llama.cpp, LM Studio and
/// OpenRouter ids. Anything printable without whitespace or control
/// characters is accepted; there is no blocklist.
pub fn is_usable_custom_model(id: &str) -> bool {
    let canonical = id.trim();
    if canonical.is_empty() || canonical.len() > 128 {
        return false;
    }
    canonical
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | ':' | '/'))
}

pub fn is_usable_model_for(provider: AiProvider, id: &str) -> bool {
    match provider {
        AiProvider::Gemini => super::is_usable_model(id),
        AiProvider::Zen | AiProvider::Go => is_usable_opencode_model(id),
        AiProvider::Custom => is_usable_custom_model(id),
    }
}

pub fn canonical_model_id_for(provider: AiProvider, id: &str) -> String {
    match provider {
        AiProvider::Gemini => super::canonical_model_id(id),
        AiProvider::Zen | AiProvider::Go => canonical_opencode_id(id),
        AiProvider::Custom => id.trim().to_owned(),
    }
}

pub fn normalize_model_for(provider: AiProvider, id: &str) -> String {
    match provider {
        AiProvider::Gemini => super::normalize_model(id),
        AiProvider::Zen | AiProvider::Go | AiProvider::Custom => {
            let canonical = canonical_model_id_for(provider, id);
            if is_usable_model_for(provider, &canonical) {
                canonical
            } else {
                provider.default_model().to_owned()
            }
        }
    }
}

/// Ordered failover queue: canonicalize, drop unusable and duplicate ids
/// (first occurrence wins), cap at [`super::MAX_AI_MODELS`], and fall back
/// to the provider default when nothing usable remains.
pub fn normalize_model_list_for(provider: AiProvider, ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut models: Vec<String> = Vec::new();
    for id in ids {
        if models.len() >= super::MAX_AI_MODELS {
            break;
        }
        let canonical = canonical_model_id_for(provider, id);
        if canonical.is_empty()
            || !seen.insert(canonical.clone())
            || !is_usable_model_for(provider, &canonical)
        {
            continue;
        }
        models.push(canonical);
    }
    if models.is_empty() {
        models.push(provider.default_model().to_owned());
    }
    models
}

/// Normalize a custom base URL: trim, drop trailing slashes, require an
/// `http(s)` scheme. Returns `None` when empty so callers fall back to the
/// Ollama default.
pub fn normalize_base_url(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() > 512
        || trimmed.chars().any(char::is_whitespace)
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

/// Turn a bare gateway id into a readable picker label
/// (`kimi-k2.7-code` → `Kimi K2.7 Code`).
pub fn prettify_model_id(id: &str) -> String {
    let mut label = String::with_capacity(id.len());
    let mut capitalize = true;
    for c in id.chars() {
        if c == '-' || c == '_' {
            label.push(' ');
            capitalize = true;
        } else if capitalize && c.is_ascii_alphabetic() {
            label.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            label.push(c);
            capitalize = false;
        }
    }
    label
}

/// Curated fallback rows: (id, label, blurb without pricing).
/// Pricing is attached from the price tables so cost edits stay in one place.
const ZEN_CURATED: &[(&str, &str, &str)] = &[
    (
        "gemini-3.8-flash",
        "Gemini 3.8 Flash",
        "Default. Same fast model as the Gemini provider.",
    ),
    (
        "glm-5.3-flash",
        "GLM 5.3 Flash",
        "Cheapest pay-as-you-go coding model.",
    ),
    (
        "kimi-k2.7-code",
        "Kimi K2.7 Code",
        "Strong open coding model, good default for Zen.",
    ),
    (
        "deepseek-v4-flash",
        "DeepSeek V4 Flash",
        "Fast budget reasoning for everyday edits.",
    ),
    (
        "qwen3.7-plus",
        "Qwen 3.7 Plus",
        "Balanced quality for longer rewrites.",
    ),
    (
        "minimax-m3",
        "MiniMax M3",
        "Capable all-rounder for writing tasks.",
    ),
    (
        "gpt-5.4-nano",
        "GPT 5.4 Nano",
        "Tiny OpenAI model for quick cleanup.",
    ),
    (
        "gpt-5.6-luna",
        "GPT 5.6 Luna",
        "Efficient OpenAI model with low rates.",
    ),
    (
        "claude-haiku-4-5",
        "Claude Haiku 4.5",
        "Fast Anthropic model for short tasks.",
    ),
    (
        "big-pickle",
        "Big Pickle",
        "Free stealth model, limited time.",
    ),
];

const GO_CURATED: &[(&str, &str, &str)] = &[
    (
        "glm-5.3-flash",
        "GLM 5.3 Flash",
        "Recommended. Fast, high-throughput model for short writing tasks.",
    ),
    (
        "qwen3.8-flash",
        "Qwen 3.8 Flash",
        "Fast alternative with a low monthly usage cost.",
    ),
    (
        "deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        "Fast budget alternative for everyday edits.",
    ),
    (
        "kimi-k2.7-code",
        "Kimi K2.7 Code",
        "Strong coding model; prose edits may take longer.",
    ),
    (
        "deepseek-v4-flash",
        "DeepSeek V4 Flash",
        "Fast budget reasoning for everyday edits.",
    ),
    (
        "qwen3.7-plus",
        "Qwen 3.7 Plus",
        "Balanced quality for longer rewrites.",
    ),
    (
        "minimax-m3",
        "MiniMax M3",
        "Capable all-rounder for writing tasks.",
    ),
    (
        "mimo-v2.5",
        "MiMo V2.5",
        "Very high request allowance per dollar.",
    ),
    (
        "muse-spark-1.3-contributor",
        "Muse Spark 1.3",
        "Meta contributor tier; trains on prompts.",
    ),
    (
        "gpt-5.6-luna",
        "GPT 5.6 Luna",
        "Efficient OpenAI model on Go.",
    ),
    ("grok-4.6", "Grok 4.6", "xAI flagship, smaller allowance."),
    (
        "union-alpha",
        "Union Alpha",
        "Free stealth model, limited time.",
    ),
];

const CUSTOM_CURATED: &[(&str, &str, &str)] = &[
    (
        "llama3.1",
        "Llama 3.1",
        "Ollama default example — `ollama pull llama3.1` first.",
    ),
    (
        "qwen2.5-coder:7b",
        "Qwen 2.5 Coder 7B",
        "Good local coding model — `ollama pull qwen2.5-coder:7b`.",
    ),
    (
        "gemma-3n-e4b",
        "Gemma 3n E4B",
        "Small local model, e.g. via LM Studio.",
    ),
];

fn curated_with_pricing(
    provider: AiProvider,
    rows: &[(&str, &str, &str)],
    billing: Billing,
    priced_description: fn(&str, Option<ModelPrice>, Option<f64>) -> String,
) -> Vec<ListedAiModel> {
    rows.iter()
        .map(|(id, label, blurb)| {
            let price = price_for(provider, id);
            let monthly = price.and_then(|price| price.monthly_limit_usd);
            ListedAiModel {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                description: priced_description(blurb, price, monthly),
                cost: cost_label_for(provider, id),
                input_per_1m: price.map(|price| price.input_per_1m),
                output_per_1m: price.map(|price| price.output_per_1m),
                monthly_limit_usd: monthly,
                billing: Some(billing.as_str().to_owned()),
            }
        })
        .collect()
}

fn zen_description(blurb: &str, price: Option<ModelPrice>, _monthly: Option<f64>) -> String {
    match price {
        Some(_) => blurb.to_owned(),
        None => blurb.to_owned(),
    }
}

fn go_description(blurb: &str, _price: Option<ModelPrice>, _monthly: Option<f64>) -> String {
    blurb.to_owned()
}

pub fn curated_zen_models() -> Vec<ListedAiModel> {
    curated_with_pricing(
        AiProvider::Zen,
        ZEN_CURATED,
        Billing::ZenCredits,
        zen_description,
    )
    .into_iter()
    .map(|mut model| {
        // Free trial models report the Free billing instead.
        if billing_for(AiProvider::Zen, &model.id) == Billing::Free {
            model.billing = Some(Billing::Free.as_str().to_owned());
        }
        model
    })
    .collect()
}

pub fn curated_go_models() -> Vec<ListedAiModel> {
    curated_with_pricing(
        AiProvider::Go,
        GO_CURATED,
        Billing::GoSubscription,
        go_description,
    )
    .into_iter()
    .map(|mut model| {
        if billing_for(AiProvider::Go, &model.id) == Billing::Free {
            model.billing = Some(Billing::Free.as_str().to_owned());
        }
        model
    })
    .collect()
}

pub fn curated_custom_models() -> Vec<ListedAiModel> {
    CUSTOM_CURATED
        .iter()
        .map(|(id, label, blurb)| ListedAiModel {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            description: (*blurb).to_owned(),
            cost: cost_label_for(AiProvider::Custom, id),
            input_per_1m: None,
            output_per_1m: None,
            monthly_limit_usd: None,
            billing: Some(Billing::Local.as_str().to_owned()),
        })
        .collect()
}

pub fn curated_models_for(provider: AiProvider) -> Vec<ListedAiModel> {
    match provider {
        AiProvider::Gemini => super::curated_listed_models(),
        AiProvider::Zen => curated_zen_models(),
        AiProvider::Go => curated_go_models(),
        AiProvider::Custom => curated_custom_models(),
    }
}

/// Map a live OpenAI-style id to a picker entry with pricing attached.
pub fn listed_opencode_model(provider: AiProvider, id: &str) -> Option<ListedAiModel> {
    if !is_usable_model_for(provider, id) {
        return None;
    }
    let canonical = canonical_model_id_for(provider, id);
    let billing = billing_for(provider, &canonical);
    let price = price_for(provider, &canonical);
    let label =
        curated_label_for(provider, &canonical).unwrap_or_else(|| prettify_model_id(&canonical));
    let blurb = curated_blurb_for(provider, &canonical);
    let description = match (provider, price, billing) {
        (_, _, Billing::Free) => blurb
            .map(str::to_owned)
            .unwrap_or_else(|| "Free for a limited time.".into()),
        (AiProvider::Go, _, _) => blurb
            .map(str::to_owned)
            .unwrap_or_else(|| "Included in the Go subscription allowance.".into()),
        (AiProvider::Zen, _, _) => blurb
            .map(str::to_owned)
            .unwrap_or_else(|| "Billed per token from Zen credits.".into()),
        (AiProvider::Custom, _, _) => blurb
            .map(str::to_owned)
            .unwrap_or_else(|| "Model served by the custom endpoint.".into()),
        _ => blurb.map(str::to_owned).unwrap_or_default(),
    };
    Some(ListedAiModel {
        id: canonical.clone(),
        label,
        description,
        cost: cost_label_for(provider, &canonical),
        input_per_1m: price.map(|price| price.input_per_1m),
        output_per_1m: price.map(|price| price.output_per_1m),
        monthly_limit_usd: price.and_then(|price| price.monthly_limit_usd),
        billing: Some(billing.as_str().to_owned()),
    })
}

fn curated_label_for(provider: AiProvider, id: &str) -> Option<String> {
    let rows = match provider {
        AiProvider::Zen => ZEN_CURATED,
        AiProvider::Go => GO_CURATED,
        AiProvider::Custom => CUSTOM_CURATED,
        AiProvider::Gemini => return None,
    };
    rows.iter()
        .find(|(known, _, _)| *known == id)
        .map(|(_, label, _)| (*label).to_owned())
}

fn curated_blurb_for(provider: AiProvider, id: &str) -> Option<&'static str> {
    let rows = match provider {
        AiProvider::Zen => ZEN_CURATED,
        AiProvider::Go => GO_CURATED,
        AiProvider::Custom => CUSTOM_CURATED,
        AiProvider::Gemini => return None,
    };
    rows.iter()
        .find(|(known, _, _)| *known == id)
        .map(|(_, _, blurb)| *blurb)
}

/// Sort with the provider default first, then free models, then by label —
/// so the cheapest way to try a provider is visible at the top.
pub fn sort_listed_models(models: &mut [ListedAiModel], provider: AiProvider) {
    let default = provider.default_model();
    models.sort_by(|a, b| {
        let a_default = a.id == default;
        let b_default = b.id == default;
        b_default
            .cmp(&a_default)
            .then_with(|| {
                let a_free = a.billing.as_deref() == Some("free");
                let b_free = b.billing.as_deref() == Some("free");
                b_free.cmp(&a_free)
            })
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
}

// ---------------------------------------------------------------------------
// OpenAI-compatible client (Zen, Go, Custom)
// ---------------------------------------------------------------------------

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
        matches!(
            code,
            Some("authentication" | "invalid_api_key" | "AuthError" | "unauthorized")
        )
    }

    pub fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidApiKey => "The API key is invalid.",
            Self::Incomplete => {
                "The provider stopped before completing the response. Try again or choose another model."
            }
            Self::InaccessibleSource => {
                "Link summaries need the Gemini provider. Paste the text or transcript instead."
            }
            Self::Api { status, code }
                if *status == StatusCode::UNAUTHORIZED
                    || *status == StatusCode::FORBIDDEN
                    || Self::is_auth_code(code.as_deref()) =>
            {
                "Couldn't connect. Check your API key."
            }
            _ if self.is_out_of_credits() => "The OpenCode balance is empty. Top up to continue.",
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
                "The provider is temporarily rate limited. Try again shortly."
            }
            Self::Api { .. } if self.is_not_found() => {
                "That model isn't available. Choose another model under AI → Model."
            }
            Self::Transport(_) => "Couldn't reach the AI provider. Check your connection.",
            _ => "The AI provider couldn't complete that request.",
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
                if *status == StatusCode::UNAUTHORIZED
                    || *status == StatusCode::FORBIDDEN
                    || Self::is_auth_code(code.as_deref()) =>
            {
                "invalid_api_key"
            }
            Self::Api { .. } if self.is_not_found() => "model_not_found",
            Self::Api { .. } => "api_error",
            Self::EmptyResponse => "empty_response",
        }
    }
}

impl fmt::Display for OpencodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.user_message())
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

/// Failures shared by every queue entry: the key is wrong, the balance is
/// empty, or the host is unreachable, so trying the next model cannot help
/// (and must not burn quota).
fn is_failover_terminal(error: &OpencodeError) -> bool {
    match error {
        OpencodeError::InvalidApiKey => true,
        OpencodeError::Transport(error) => !error.is_timeout(),
        OpencodeError::Api { status, code }
            if *status == StatusCode::UNAUTHORIZED
                || *status == StatusCode::FORBIDDEN
                || OpencodeError::is_auth_code(code.as_deref()) =>
        {
            true
        }
        _ => error.is_out_of_credits(),
    }
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

    fn chat_request(&self, provider: AiProvider, chat_url: &str) -> reqwest::RequestBuilder {
        let request = self.http.post(chat_url);
        if matches!(provider, AiProvider::Zen | AiProvider::Go) {
            // Each stateless writing operation is a separate conversation. Identify
            // Kivo honestly and provide the routing header required by Go.
            static NEXT_SESSION: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let sequence = NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let session = format!(
                "kivo-{}-{}-{sequence}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            request
                .header("user-agent", concat!("Kivo/", env!("CARGO_PKG_VERSION")))
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
        let status = response.status();
        if !status.is_success() {
            let error_code = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|body| parse_openai_error_code(&body));
            return Err(OpencodeError::Api {
                status,
                code: error_code,
            });
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

    pub async fn generate(
        &self,
        provider: AiProvider,
        chat_url: &str,
        api_key: Option<&SecretString>,
        model: &str,
        prompt: &AiPrompt,
    ) -> Result<String, OpencodeError> {
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
        let (url, body) = completion_request(protocol, chat_url, &request);
        let mut call = self.chat_request(provider, &url).json(&body);
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
            // OpenCode's Zen Google adapter follows the Google SDK contract
            // and expects the key in x-goog-api-key, unlike its OpenAI and
            // Anthropic adapters.
            let header =
                HeaderValue::from_str(key.expose()).map_err(|_| OpencodeError::InvalidApiKey)?;
            call = call.header("x-goog-api-key", header);
        }
        let response = call.send().await.map_err(OpencodeError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            let error_code = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|body| parse_openai_error_code(&body));
            return Err(OpencodeError::Api {
                status,
                code: error_code,
            });
        }
        let completion = response
            .json::<serde_json::Value>()
            .await
            .map_err(OpencodeError::InvalidResponse)?;
        parse_completion(protocol, completion)
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
            max_tokens: Some(1),
        };
        let protocol = completion_protocol(provider, &model);
        let (url, body) = completion_request(protocol, chat_url, &request);
        let mut call = self.chat_request(provider, &url).json(&body);
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
        let status = response.status();
        if !status.is_success() {
            let error_code = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|body| parse_openai_error_code(&body));
            return Err(OpencodeError::Api {
                status,
                code: error_code,
            });
        }
        Ok(())
    }

    /// Try each model in order until one succeeds. Key/account failures
    /// (auth, empty balance) abort immediately: every entry shares the same
    /// key, so continuing could only burn quota. Anything else — rate
    /// limits, unknown model ids, server errors, empty responses — falls
    /// through to the next model, and the last error is returned when all
    /// fail.
    pub async fn generate_in_order(
        &self,
        provider: AiProvider,
        chat_url: &str,
        api_key: Option<&SecretString>,
        models: &[String],
        prompt: &AiPrompt,
    ) -> Result<String, OpencodeError> {
        let normalized = normalize_model_list_for(provider, models);
        let mut last_error: Option<OpencodeError> = None;
        for model in &normalized {
            match self
                .generate(provider, chat_url, api_key, model, prompt)
                .await
            {
                Ok(output) => return Ok(output),
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

fn completion_request(
    protocol: CompletionProtocol,
    chat_url: &str,
    request: &ChatCompletionRequest<'_>,
) -> (String, serde_json::Value) {
    let base = chat_url
        .strip_suffix("/chat/completions")
        .unwrap_or(chat_url);
    match protocol {
        CompletionProtocol::Chat => (
            chat_url.to_owned(),
            serde_json::to_value(request).expect("serializable request"),
        ),
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
            (
                format!("{base}/messages"),
                serde_json::json!({
                    "model": request.model,
                    "system": system,
                    "messages": messages,
                    // Anthropic's protocol requires an explicit output limit.
                    "max_tokens": request.max_tokens.unwrap_or(8192),
                }),
            )
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

fn parse_completion(
    protocol: CompletionProtocol,
    body: serde_json::Value,
) -> Result<String, OpencodeError> {
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
        return parse_chat_completion(response);
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
        Ok(output.to_owned())
    }
}

#[derive(Serialize)]
// Omit temperature: models such as Kimi only accept their default sampling settings.
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage<'a>],
    // No output cap on writing requests: prompt budgets were dropped alongside
    // the Gemini `max_output_tokens` (same reasoning — small sources produce
    // small outputs). Only the connection probe caps to one token.
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

/// Normalize the OpenAI-style error payload into a short machine-readable
/// code. Gateways answer `{"error": {"message": …, "type": "AuthError"}}`
/// (Zen/Go use `AuthError` for bad keys) or the OpenAI
/// `{"error": {"code": "model_not_found"}}` shape.
fn parse_openai_error_code(body: &serde_json::Value) -> Option<String> {
    let error = body.get("error").unwrap_or(body);
    if let Some(text) = error.as_str() {
        return Some(text.to_owned());
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

    if matches!(kind, Some("AuthError"))
        || message_lower.contains("invalid api key")
        || message_lower.contains("incorrect api key")
        || message_lower.contains("unauthorized")
    {
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
    code.or_else(|| kind.map(str::to_owned))
        .or_else(|| (!message.is_empty()).then(|| "api_error".to_owned()))
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn go_requests_identify_kivo_and_use_model_sampling_defaults() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut sessions = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break bytes.len();
                    }
                };
                let headers = String::from_utf8(bytes.clone()).unwrap().to_lowercase();
                assert!(headers.contains("user-agent: kivo/"));
                let session = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("x-opencode-session: "))
                    .unwrap();
                assert!(session.starts_with("kivo-"));
                sessions.push(session.to_owned());
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                bytes.resize(header_end + length, 0);
                stream.read_exact(&mut bytes[header_end..]).unwrap();
                let body: serde_json::Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
                assert_eq!(body["model"], "kimi-k2.7-code");
                assert!(body.get("temperature").is_none());
                let response = r#"{"choices":[{"message":{"content":"hello world"}}]}"#;
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
            assert_ne!(sessions[0], sessions[1]);
        });
        let client = OpenAiCompatClient::new().unwrap();
        let key = SecretString::new("test-key".to_owned()).unwrap();
        client
            .test_connection(AiProvider::Go, &url, Some(&key), "kimi-k2.7-code")
            .await
            .unwrap();
        let prompt = AiPrompt {
            system_instruction: "Correct spelling".into(),
            input: "helo world".into(),
        };
        assert_eq!(
            client
                .generate(AiProvider::Go, &url, Some(&key), "kimi-k2.7-code", &prompt)
                .await
                .unwrap(),
            "hello world"
        );
        server.join().unwrap();
        let custom = client
            .chat_request(AiProvider::Custom, &url)
            .build()
            .unwrap();
        assert!(!custom.headers().contains_key("x-opencode-session"));
    }

    #[test]
    fn provider_ids_parse_and_round_trip() {
        assert_eq!(AiProvider::parse("gemini"), AiProvider::Gemini);
        assert_eq!(AiProvider::parse("zen"), AiProvider::Zen);
        assert_eq!(AiProvider::parse("opencode"), AiProvider::Zen);
        assert_eq!(AiProvider::parse("opencode-go"), AiProvider::Go);
        assert_eq!(AiProvider::parse("go"), AiProvider::Go);
        assert_eq!(AiProvider::parse("custom"), AiProvider::Custom);
        assert_eq!(AiProvider::parse("ollama"), AiProvider::Custom);
        assert_eq!(AiProvider::parse(""), AiProvider::Gemini);
        assert_eq!(AiProvider::parse("unknown"), AiProvider::Gemini);
        for provider in AiProvider::all() {
            assert_eq!(AiProvider::parse(provider.as_str()), *provider);
        }
    }

    #[test]
    fn provider_metadata_covers_all_backends() {
        let infos = provider_infos();
        assert_eq!(infos.len(), AiProvider::all().len());
        let zen = infos.iter().find(|info| info.id == "zen").unwrap();
        assert_eq!(zen.label, "OpenCode Zen");
        assert_eq!(zen.key_url.as_deref(), Some("https://opencode.ai/auth"));
        assert!(!zen.key_optional);
        assert!(!zen.supports_link_summary);
        assert!(zen.test_uses_quota);
        let gemini = infos.iter().find(|info| info.id == "gemini").unwrap();
        assert!(gemini.supports_link_summary);
        assert!(gemini.test_uses_quota);
        let custom = infos.iter().find(|info| info.id == "custom").unwrap();
        assert!(custom.key_optional);
        assert_eq!(
            custom.default_base_url.as_deref(),
            Some("http://localhost:11434/v1")
        );
    }

    #[test]
    fn credential_slots_are_stable_per_provider() {
        // The Gemini slot must never change: existing installs keep their key.
        assert_eq!(AiProvider::Gemini.credential_account(), "gemini-api-key");
        assert_eq!(AiProvider::Zen.credential_account(), "opencode-zen-api-key");
        assert_eq!(AiProvider::Go.credential_account(), "opencode-go-api-key");
        assert_eq!(
            AiProvider::Custom.credential_account(),
            "opencode-custom-api-key"
        );
    }

    #[test]
    fn endpoints_resolve_per_provider() {
        assert_eq!(
            AiProvider::Zen.resolve_models_url(None).as_deref(),
            Some("https://opencode.ai/zen/v1/models")
        );
        assert_eq!(
            AiProvider::Go.resolve_chat_url(None).as_deref(),
            Some("https://opencode.ai/zen/go/v1/chat/completions")
        );
        assert_eq!(AiProvider::Gemini.resolve_chat_url(None), None);
        // Custom joins the base URL; empty falls back to Ollama.
        assert_eq!(
            AiProvider::Custom.resolve_models_url(None).as_deref(),
            Some("http://localhost:11434/v1/models")
        );
        assert_eq!(
            AiProvider::Custom
                .resolve_chat_url(Some("http://127.0.0.1:1234/v1/"))
                .as_deref(),
            Some("http://127.0.0.1:1234/v1/chat/completions")
        );
        assert_eq!(
            AiProvider::Custom.resolve_models_url(Some("  ")).as_deref(),
            Some("http://localhost:11434/v1/models")
        );
    }

    #[test]
    fn base_urls_normalize_safely() {
        assert_eq!(normalize_base_url(None), None);
        assert_eq!(normalize_base_url(Some("  ")), None);
        assert_eq!(
            normalize_base_url(Some("http://localhost:11434/v1/")),
            Some("http://localhost:11434/v1".into())
        );
        assert_eq!(normalize_base_url(Some("notaurl")), None);
        assert_eq!(normalize_base_url(Some("ftp://host/v1")), None);
        assert_eq!(normalize_base_url(Some("https://x y/v1")), None);
        assert_eq!(
            AiProvider::Custom.resolve_chat_url(None).as_deref(),
            Some("http://localhost:11434/v1/chat/completions")
        );
    }

    #[test]
    fn opencode_ids_validate_and_strip_prefixes() {
        assert_eq!(canonical_opencode_id("opencode/gpt-5.5"), "gpt-5.5");
        assert_eq!(canonical_opencode_id("opencode-go/kimi-k3"), "kimi-k3");
        assert!(is_usable_opencode_model("kimi-k2.7-code"));
        assert!(is_usable_opencode_model("qwen3.7-plus"));
        assert!(is_usable_opencode_model("opencode/gpt-5.5"));
        assert!(!is_usable_opencode_model("gpt"));
        assert!(!is_usable_opencode_model("has spaces!"));
        assert!(!is_usable_opencode_model("UPPER-CASE"));
        assert_eq!(
            normalize_model_for(AiProvider::Zen, "opencode/gpt-5.5"),
            "gpt-5.5"
        );
        assert_eq!(
            normalize_model_for(AiProvider::Go, "bogus!!"),
            "glm-5.3-flash"
        );
    }

    #[test]
    fn custom_ids_accept_local_shapes() {
        assert!(is_usable_custom_model("llama3.1"));
        assert!(is_usable_custom_model("llama2"));
        assert!(is_usable_custom_model("google/gemma-3n-e4b"));
        assert!(is_usable_custom_model("qwen2.5-coder:7b"));
        assert!(!is_usable_custom_model(""));
        assert!(!is_usable_custom_model("has spaces"));
        assert_eq!(
            normalize_model_for(AiProvider::Custom, "  llama3.1  "),
            "llama3.1"
        );
    }

    #[test]
    fn cost_labels_help_compare_models() {
        assert_eq!(
            cost_label_for(AiProvider::Zen, "glm-5.3-flash").as_deref(),
            Some("$0.15 in / $0.50 out per 1M")
        );
        assert_eq!(
            cost_label_for(AiProvider::Go, "kimi-k2.7-code").as_deref(),
            Some("$0.95 in / $4.00 out per 1M · $60/mo incl.")
        );
        assert_eq!(
            cost_label_for(AiProvider::Zen, "big-pickle").as_deref(),
            Some("Free")
        );
        assert_eq!(
            cost_label_for(AiProvider::Custom, "llama3.1").as_deref(),
            Some("Local · free")
        );
        // Unknown future ids still get a usable entry with generic billing.
        let unknown = listed_opencode_model(AiProvider::Zen, "future-model-x").unwrap();
        assert_eq!(unknown.label, "Future Model X");
        assert_eq!(unknown.billing.as_deref(), Some("zen_credits"));
    }

    #[test]
    fn curated_fallbacks_carry_pricing() {
        for provider in [AiProvider::Zen, AiProvider::Go, AiProvider::Custom] {
            let curated = curated_models_for(provider);
            assert!(!curated.is_empty());
            assert!(curated.iter().all(|model| model.cost.is_some()));
            assert!(curated.iter().all(|model| model.billing.is_some()));
            assert!(
                curated
                    .iter()
                    .any(|model| model.id == provider.default_model())
            );
        }
    }

    #[test]
    fn model_lists_dedupe_truncate_and_fall_back() {
        use super::super::MAX_AI_MODELS;
        // Prefixes canonicalize, duplicates collapse, junk drops out.
        let list = normalize_model_list_for(
            AiProvider::Zen,
            &[
                "opencode/kimi-k2.7-code".to_owned(),
                "kimi-k2.7-code".to_owned(),
                "bogus!!".to_owned(),
                "glm-5.3-flash".to_owned(),
            ],
        );
        assert_eq!(list, vec!["kimi-k2.7-code", "glm-5.3-flash"]);
        // Empty / fully unusable lists fall back to the provider default.
        assert_eq!(
            normalize_model_list_for(AiProvider::Go, &[]),
            vec![AiProvider::Go.default_model().to_owned()]
        );
        assert_eq!(
            normalize_model_list_for(AiProvider::Go, &["bogus!!".to_owned()]),
            vec![AiProvider::Go.default_model().to_owned()]
        );
        // Long lists truncate to the cap.
        let many: Vec<String> = (0..MAX_AI_MODELS + 3)
            .map(|n| format!("model-{n}-x"))
            .collect();
        let list = normalize_model_list_for(AiProvider::Zen, &many);
        assert_eq!(list.len(), MAX_AI_MODELS);
        assert_eq!(list[0], "model-0-x");
    }

    #[test]
    fn openai_error_shapes_map_to_indicator_codes() {
        // Zen/Go invalid key: {"error": {"type": "AuthError", ...}} with 401.
        let auth =
            serde_json::json!({"error": {"type": "AuthError", "message": "Invalid API key."}});
        assert_eq!(
            parse_openai_error_code(&auth).as_deref(),
            Some("authentication")
        );
        let error = OpencodeError::Api {
            status: StatusCode::UNAUTHORIZED,
            code: parse_openai_error_code(&auth),
        };
        assert_eq!(error.code(), "invalid_api_key");

        // Empty Zen balance.
        let empty =
            serde_json::json!({"error": {"message": "Insufficient balance, please top up."}});
        let error = OpencodeError::Api {
            status: StatusCode::BAD_REQUEST,
            code: parse_openai_error_code(&empty),
        };
        assert!(error.is_out_of_credits());
        assert_eq!(error.code(), "insufficient_credits");

        // Unknown model.
        let missing =
            serde_json::json!({"error": {"code": "model_not_found", "message": "No such model."}});
        let error = OpencodeError::Api {
            status: StatusCode::NOT_FOUND,
            code: parse_openai_error_code(&missing),
        };
        assert!(error.is_not_found());
        assert_eq!(error.code(), "model_not_found");

        // Rate limit.
        let limited =
            serde_json::json!({"error": {"code": "rate_limit_exceeded", "message": "Slow down."}});
        let error = OpencodeError::Api {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: parse_openai_error_code(&limited),
        };
        assert!(error.is_rate_limited());
        assert_eq!(error.code(), "rate_limited");
    }

    #[test]
    fn chat_completions_parse_text_and_parts() {
        let text = serde_json::from_str::<ChatCompletionResponse>(
            r#"{"choices":[{"message":{"content":"Hello, world."}}]}"#,
        )
        .unwrap();
        assert_eq!(parse_chat_completion(text).unwrap(), "Hello, world.");
        let parts = serde_json::from_str::<ChatCompletionResponse>(
            r#"{"choices":[{"message":{"content":[{"type":"text","text":"Hi "},{"type":"reasoning","text":"hidden"},{"type":"text","text":"there."}]}}]}"#,
        )
        .unwrap();
        assert_eq!(parse_chat_completion(parts).unwrap(), "Hi there.");
        let empty = serde_json::from_str::<ChatCompletionResponse>(r#"{"choices":[]}"#).unwrap();
        assert!(parse_chat_completion(empty).is_err());
    }

    #[test]
    fn hosted_models_use_their_required_protocol_without_changing_custom_servers() {
        for model in ["minimax-m2.7", "qwen3.5-plus", "union-alpha"] {
            assert_eq!(
                completion_protocol(AiProvider::Go, model),
                CompletionProtocol::Messages
            );
            assert_eq!(
                completion_protocol(AiProvider::Zen, model),
                if model.starts_with("qwen") {
                    CompletionProtocol::Messages
                } else {
                    CompletionProtocol::Chat
                }
            );
            assert_eq!(
                completion_protocol(AiProvider::Custom, model),
                CompletionProtocol::Chat
            );
        }
        assert_eq!(
            completion_protocol(AiProvider::Zen, "claude-sonnet-4-6"),
            CompletionProtocol::Messages
        );
        for model in ["gpt-5.6-luna", "grok-4.5", "muse-spark"] {
            assert_eq!(
                completion_protocol(AiProvider::Go, model),
                CompletionProtocol::Responses
            );
            assert_eq!(
                completion_protocol(AiProvider::Zen, model),
                CompletionProtocol::Responses
            );
            assert_eq!(
                completion_protocol(AiProvider::Custom, model),
                CompletionProtocol::Chat
            );
        }
        for model in ["gemini-3.8-flash", "gemini-3.5-flash"] {
            assert_eq!(
                completion_protocol(AiProvider::Zen, model),
                CompletionProtocol::Google
            );
            assert_eq!(
                completion_protocol(AiProvider::Go, model),
                CompletionProtocol::Chat
            );
        }
        assert_eq!(
            completion_protocol(AiProvider::Go, "kimi-k2.7-code"),
            CompletionProtocol::Chat
        );
    }

    #[test]
    fn messages_requests_preserve_instructions_and_supply_required_output_limit() {
        let messages = [
            ChatMessage {
                role: "system",
                content: "Preserve meaning.",
            },
            ChatMessage {
                role: "user",
                content: "helo world",
            },
        ];
        let request = ChatCompletionRequest {
            model: "minimax-m2.7",
            messages: &messages,
            max_tokens: None,
        };
        let (url, body) = completion_request(
            CompletionProtocol::Messages,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &request,
        );
        assert_eq!(url, "https://opencode.ai/zen/go/v1/messages");
        assert_eq!(body["model"], "minimax-m2.7");
        assert_eq!(body["system"], "Preserve meaning.");
        assert_eq!(
            body["messages"],
            serde_json::json!([{"role":"user", "content":"helo world"}])
        );
        assert_eq!(body["max_tokens"], 8192);
        assert!(body.get("temperature").is_none());
        let probe = ChatCompletionRequest {
            max_tokens: Some(1),
            ..request
        };
        let (_, body) = completion_request(
            CompletionProtocol::Messages,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &probe,
        );
        assert_eq!(body["max_tokens"], 1);
    }

    #[test]
    fn responses_requests_are_stateless_and_use_protocol_specific_probe_limit() {
        let messages = [
            ChatMessage {
                role: "system",
                content: "Preserve meaning.",
            },
            ChatMessage {
                role: "user",
                content: "helo world",
            },
        ];
        let request = ChatCompletionRequest {
            model: "gpt-5.6-luna",
            messages: &messages,
            max_tokens: None,
        };
        let (url, body) = completion_request(
            CompletionProtocol::Responses,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &request,
        );
        assert_eq!(url, "https://opencode.ai/zen/go/v1/responses");
        assert_eq!(body["model"], "gpt-5.6-luna");
        assert_eq!(
            body["input"],
            serde_json::json!([
                {"role":"system", "content":"Preserve meaning."},
                {"role":"user", "content":"helo world"}
            ])
        );
        assert_eq!(body["store"], false);
        assert!(body.get("max_output_tokens").is_none());
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
        let probe = ChatCompletionRequest {
            max_tokens: Some(1),
            ..request
        };
        let (_, body) = completion_request(
            CompletionProtocol::Responses,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &probe,
        );
        assert_eq!(body["max_output_tokens"], 16);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn google_requests_use_the_zen_generate_content_shape() {
        let messages = [
            ChatMessage {
                role: "system",
                content: "Preserve meaning.",
            },
            ChatMessage {
                role: "user",
                content: "helo world",
            },
        ];
        let request = ChatCompletionRequest {
            model: "gemini-3.8-flash",
            messages: &messages,
            max_tokens: Some(1),
        };
        let (url, body) = completion_request(
            CompletionProtocol::Google,
            "https://opencode.ai/zen/v1/chat/completions",
            &request,
        );
        assert_eq!(
            url,
            "https://opencode.ai/zen/v1/models/gemini-3.8-flash:generateContent"
        );
        assert_eq!(
            body["systemInstruction"],
            serde_json::json!({"parts":[{"text":"Preserve meaning."}]})
        );
        assert_eq!(
            body["contents"],
            serde_json::json!([{"role":"user","parts":[{"text":"helo world"}]}])
        );
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 1);
        let response = serde_json::json!({
            "candidates": [{
                "content": {"parts": [{"text": "hello world"}]},
                "finishReason": "STOP"
            }]
        });
        assert_eq!(
            parse_completion(CompletionProtocol::Google, response).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn protocol_parsers_return_only_visible_answer_text() {
        let messages = serde_json::json!({"content":[
            {"type":"thinking", "thinking":"secret", "text":"Do not reveal"},
            {"type":"text", "text":" Hello, "},
            {"type":"tool_use", "text":"Not the answer"},
            {"type":"text", "text":"world. "}
        ]});
        assert_eq!(
            parse_completion(CompletionProtocol::Messages, messages).unwrap(),
            "Hello, world."
        );
        let responses = serde_json::json!({"status":"completed", "output":[
            {"type":"reasoning", "content":[{"type":"output_text", "text":"Do not reveal"}]},
            {"type":"message", "content":[
                {"type":"output_text", "text":" Hello, "},
                {"type":"reasoning_text", "text":"secret"},
                {"type":"output_text", "text":"world. "}
            ]}
        ]});
        assert_eq!(
            parse_completion(CompletionProtocol::Responses, responses).unwrap(),
            "Hello, world."
        );
        for (protocol, body) in [
            (
                CompletionProtocol::Messages,
                serde_json::json!({"content":[{"type":"thinking", "thinking":"secret"}]}),
            ),
            (
                CompletionProtocol::Responses,
                serde_json::json!({"output":[{"type":"reasoning", "summary":[]}]}),
            ),
            (
                CompletionProtocol::Messages,
                serde_json::json!({"content":"malformed"}),
            ),
            (
                CompletionProtocol::Responses,
                serde_json::json!({"output":null}),
            ),
        ] {
            assert!(matches!(
                parse_completion(protocol, body),
                Err(OpencodeError::EmptyResponse)
            ));
        }
    }

    #[test]
    fn protocol_parsers_reject_partial_answers_before_replacing_user_text() {
        for status in ["incomplete", "failed", "cancelled", "in_progress", "queued"] {
            let body = serde_json::json!({
                "status":status,
                "output":[{"type":"message", "content":[{"type":"output_text", "text":"Partial answer"}]}]
            });
            assert!(matches!(
                parse_completion(CompletionProtocol::Responses, body),
                Err(OpencodeError::Incomplete)
            ));
        }
        for stop_reason in ["max_tokens", "pause_turn"] {
            let body = serde_json::json!({
                "stop_reason":stop_reason,
                "content":[{"type":"text", "text":"Partial answer"}]
            });
            assert!(matches!(
                parse_completion(CompletionProtocol::Messages, body),
                Err(OpencodeError::Incomplete)
            ));
        }
        let chat = serde_json::json!({"choices":[{
            "finish_reason":"length", "message":{"content":"Partial answer"}
        }]});
        assert!(matches!(
            parse_completion(CompletionProtocol::Chat, chat),
            Err(OpencodeError::Incomplete)
        ));
        assert!(!is_failover_terminal(&OpencodeError::Incomplete));
    }

    #[test]
    fn invalid_request_errors_do_not_mark_valid_keys_invalid() {
        let body = serde_json::json!({
            "error": {
                "type": "invalid_request_error",
                "message": "This model does not support the requested temperature."
            }
        });
        let error = OpencodeError::Api {
            status: StatusCode::BAD_REQUEST,
            code: parse_openai_error_code(&body),
        };
        assert_eq!(error.code(), "api_error");
        assert!(!is_failover_terminal(&error));
        assert!(!error.user_message().contains("API key"));

        // The same generic error type may accompany genuine HTTP auth errors.
        let unauthorized = OpencodeError::Api {
            status: StatusCode::UNAUTHORIZED,
            code: Some("invalid_request_error".into()),
        };
        assert_eq!(unauthorized.code(), "invalid_api_key");
        assert!(is_failover_terminal(&unauthorized));
        let invalid_key = OpencodeError::Api {
            status: StatusCode::BAD_REQUEST,
            code: Some("invalid_api_key".into()),
        };
        assert_eq!(invalid_key.code(), "invalid_api_key");
        assert!(is_failover_terminal(&invalid_key));
    }

    #[test]
    fn failover_continues_past_model_errors_but_not_key_errors() {
        // Shared key/balance failures stop the queue immediately.
        assert!(is_failover_terminal(&OpencodeError::InvalidApiKey));
        assert!(is_failover_terminal(&OpencodeError::Api {
            status: StatusCode::UNAUTHORIZED,
            code: Some("authentication".into()),
        }));
        assert!(is_failover_terminal(&OpencodeError::Api {
            status: StatusCode::BAD_REQUEST,
            code: Some("insufficient_credits".into()),
        }));
        // Per-model failures fall through to the next entry.
        assert!(!is_failover_terminal(&OpencodeError::Api {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: Some("rate_limited".into()),
        }));
        assert!(!is_failover_terminal(&OpencodeError::Api {
            status: StatusCode::NOT_FOUND,
            code: Some("not_found".into()),
        }));
        assert!(!is_failover_terminal(&OpencodeError::EmptyResponse));
    }
}
