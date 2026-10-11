use super::{AiProvider, Billing, ListedAiModel};

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
    crate::ai::BLOCKED_MODEL_SUBSTRINGS
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
        AiProvider::Gemini => crate::ai::is_usable_model(id),
        AiProvider::Zen | AiProvider::Go => is_usable_opencode_model(id),
        AiProvider::Custom => is_usable_custom_model(id),
    }
}

pub fn canonical_model_id_for(provider: AiProvider, id: &str) -> String {
    match provider {
        AiProvider::Gemini => crate::ai::canonical_model_id(id),
        AiProvider::Zen | AiProvider::Go => canonical_opencode_id(id),
        AiProvider::Custom => id.trim().to_owned(),
    }
}

pub fn normalize_model_for(provider: AiProvider, id: &str) -> String {
    match provider {
        AiProvider::Gemini => crate::ai::normalize_model(id),
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
/// (first occurrence wins), cap at [`crate::ai::MAX_AI_MODELS`], and fall back
/// to the provider default when nothing usable remains.
pub fn normalize_model_list_for(provider: AiProvider, ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut models: Vec<String> = Vec::new();
    for id in ids {
        if models.len() >= crate::ai::MAX_AI_MODELS {
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
        AiProvider::Gemini => crate::ai::curated_listed_models(),
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
