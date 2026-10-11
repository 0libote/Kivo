use super::{
    AiReasoningMode, ApiModel, DEFAULT_GEMINI_MODEL, GEMINI_MODEL, GeminiError, GenerationConfig,
    InteractionRequest, InteractionResponse, WritingAction, curated_listed_models,
    dictation_cleanup_prompt, filter_api_models, is_blocked_model, is_usable_model,
    normalize_model, parse_api_error_code, parse_api_error_detail, parse_interaction,
    supported_models, thinking_level_for, writing_prompt, writing_prompt_from_system,
};

#[test]
fn writing_actions_encode_the_intended_constraints() {
    let prompt = writing_prompt(
        WritingAction::Professional,
        "hey can you send it",
        None,
        &[],
    )
    .unwrap();
    assert!(prompt.system_instruction.contains("professional"));
    assert!(prompt.system_instruction.contains("without jargon"));
    assert!(prompt.system_instruction.contains("plain text only"));
    assert!(prompt.input.contains("hey can you send it"));
}

#[test]
fn custom_system_instruction_is_used_verbatim_and_wraps_the_source() {
    let prompt =
        writing_prompt_from_system("Do exactly this.".to_owned(), "the source", &[]).unwrap();
    assert_eq!(prompt.system_instruction, "Do exactly this.");
    assert!(
        prompt
            .input
            .contains("<source_text>\nthe source\n</source_text>")
    );
    assert!(matches!(
        writing_prompt_from_system("x".to_owned(), "   ", &[]),
        Err(super::PromptError::EmptySource)
    ));
}

#[test]
fn action_ids_round_trip_and_fold_legacy_spellings() {
    for action in WritingAction::all() {
        assert_eq!(WritingAction::parse_id(action.as_str()), Some(*action));
    }
    assert_eq!(WritingAction::canonical_id("keyPoints"), "key-points");
    assert_eq!(WritingAction::canonical_id("key-points"), "key-points");
    assert_eq!(WritingAction::canonical_id("proofread"), "proofread");
    assert_eq!(WritingAction::parse_id("not-an-action"), None);
}

#[test]
fn custom_action_requires_an_instruction() {
    assert!(writing_prompt(WritingAction::Custom, "Text", Some("  "), &[]).is_err());
}

#[test]
fn informational_actions_request_markdown_without_replacement() {
    let prompt = writing_prompt(WritingAction::KeyPoints, "One. Two.", None, &[]).unwrap();
    assert!(prompt.system_instruction.contains("Markdown bullet list"));
    assert!(!WritingAction::KeyPoints.replaces_selection());
}

#[test]
fn summaries_preserve_source_details() {
    let prompt = writing_prompt(WritingAction::Summarize, "Short source.", None, &[]).unwrap();
    assert!(prompt.system_instruction.contains("short overview"));
    assert!(prompt.system_instruction.contains("source language"));
    assert!(
        prompt
            .system_instruction
            .contains("timestamps only when supplied")
    );
    let long = "a".repeat(200_000);
    assert!(writing_prompt(WritingAction::Summarize, &long, None, &[]).is_ok());
    assert!(matches!(
        writing_prompt(WritingAction::Summarize, &(long + "a"), None, &[]),
        Err(super::PromptError::SummaryTooLong)
    ));
}

#[test]
fn dictation_prompt_preserves_meaning_and_tone() {
    let prompt = dictation_cleanup_prompt("um hello hello there", &[]).unwrap();
    assert!(
        prompt
            .system_instruction
            .contains("Preserve the speaker's meaning")
    );
    assert!(
        prompt
            .system_instruction
            .contains("Do not rewrite aggressively")
    );
}

#[test]
fn vocabulary_hint_is_empty_aware_and_bounded() {
    use crate::config::VocabularyEntry;
    fn entry(word: &str, meaning: &str) -> VocabularyEntry {
        VocabularyEntry {
            word: word.into(),
            meaning: meaning.into(),
        }
    }
    assert_eq!(super::vocabulary_hint(&[]), None);
    assert_eq!(super::vocabulary_hint(&[entry("  ", "")]), None);
    let hint = super::vocabulary_hint(&[entry("Kivo", "our product"), entry("SOC 2", "")]).unwrap();
    assert!(hint.contains("Kivo (our product)"));
    assert!(hint.contains("SOC 2"));
    assert!(!hint.contains("SOC 2 ("));
    assert!(hint.contains("never \"correct\" them"));
    // Oversized lists truncate to the prompt cap.
    let many: Vec<VocabularyEntry> = (0..super::MAX_VOCABULARY_PROMPT_TERMS + 10)
        .map(|n| entry(&format!("word-{n}"), ""))
        .collect();
    let hint = super::vocabulary_hint(&many).unwrap();
    assert!(hint.contains("word-0"));
    assert!(!hint.contains(&format!("word-{}", super::MAX_VOCABULARY_PROMPT_TERMS + 9)));
}

#[test]
fn vocabulary_reaches_dictation_and_writing_prompts() {
    use crate::config::VocabularyEntry;
    let vocabulary = vec![
        VocabularyEntry {
            word: "Kivo".into(),
            meaning: "our product".into(),
        },
        VocabularyEntry {
            word: "Siobhán".into(),
            meaning: String::new(),
        },
    ];
    let prompt = dictation_cleanup_prompt("hello", &vocabulary).unwrap();
    assert!(prompt.system_instruction.contains("Kivo (our product)"));
    assert!(prompt.system_instruction.contains("Siobhán"));

    let prompt = writing_prompt(WritingAction::Proofread, "hello", None, &vocabulary).unwrap();
    assert!(prompt.system_instruction.contains("Kivo (our product)"));

    let prompt =
        writing_prompt_from_system("Do exactly this.".to_owned(), "the source", &vocabulary)
            .unwrap();
    assert!(prompt.system_instruction.starts_with("Do exactly this."));
    assert!(prompt.system_instruction.contains("Kivo (our product)"));

    // Empty vocabulary leaves prompts untouched.
    let prompt = dictation_cleanup_prompt("hello", &[]).unwrap();
    assert!(!prompt.system_instruction.contains("Custom vocabulary"));
    let prompt = writing_prompt(WritingAction::Proofread, "hello", None, &[]).unwrap();
    assert!(!prompt.system_instruction.contains("Custom vocabulary"));
}

#[test]
fn request_is_stateless_and_uses_low_thinking() {
    let request = InteractionRequest {
        model: GEMINI_MODEL,
        input: "source",
        system_instruction: "instruction",
        store: false,
        generation_config: thinking_level_for(GEMINI_MODEL, AiReasoningMode::Fast)
            .map(|thinking_level| GenerationConfig { thinking_level }),
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["store"], false);
    assert_eq!(json["model"], "gemini-3.8-flash");
    assert_eq!(json["generation_config"]["thinking_level"], "low");
    assert!(json["generation_config"].get("temperature").is_none());
    // ponytail: plain-text requests omit response_format (structured-output only).
    assert!(json.get("response_format").is_none());
}

#[test]
fn unsupported_or_variant_models_omit_thinking_level() {
    // Gemma rejects thinking_level "low" with HTTP 400 invalid_request,
    // and future/variant Gemini models may support a different set of
    // levels, so both must use the provider default.
    assert_eq!(
        thinking_level_for("gemini-3.8-flash", AiReasoningMode::Fast),
        Some("low")
    );
    assert_eq!(
        thinking_level_for("models/gemini-3.6-flash", AiReasoningMode::Balanced),
        Some("medium")
    );
    assert_eq!(
        thinking_level_for("gemini-3.8-flash", AiReasoningMode::Deep),
        Some("high")
    );
    assert_eq!(
        thinking_level_for("gemini-3-pro-preview", AiReasoningMode::Balanced),
        Some("low")
    );
    assert_eq!(
        thinking_level_for("gemini-4-flash", AiReasoningMode::Fast),
        None
    );
    assert_eq!(
        thinking_level_for("gemma-4-31b-it", AiReasoningMode::Fast),
        None
    );
    let request = InteractionRequest {
        model: "gemma-4-31b-it",
        input: "source",
        system_instruction: "instruction",
        store: false,
        generation_config: thinking_level_for("gemma-4-31b-it", AiReasoningMode::Fast)
            .map(|thinking_level| GenerationConfig { thinking_level }),
    };
    let json = serde_json::to_value(request).unwrap();
    assert!(json.get("generation_config").is_none());
}

#[test]
fn parses_text_from_model_output_steps() {
    let fixture = r#"{
      "status": "completed",
      "steps": [
        {"type":"model_output","content":[
          {"type":"text","text":"Hello, "},
          {"type":"text","text":"world."}
        ]}
      ]
    }"#;
    let response: InteractionResponse = serde_json::from_str(fixture).unwrap();
    assert_eq!(parse_interaction(response).unwrap(), "Hello, world.");
}

#[test]
fn rejects_incomplete_and_empty_responses() {
    let incomplete: InteractionResponse =
        serde_json::from_str(r#"{"status":"incomplete","steps":[]}"#).unwrap();
    assert!(parse_interaction(incomplete).is_err());

    let empty: InteractionResponse =
        serde_json::from_str(r#"{"status":"completed","steps":[]}"#).unwrap();
    assert!(parse_interaction(empty).is_err());
}

#[test]
fn model_allowlist_covers_default_and_only_text_models() {
    let models = supported_models();
    assert!(!models.is_empty());
    assert!(models.iter().any(|model| model.id == DEFAULT_GEMINI_MODEL));
    assert!(models.iter().any(|model| model.id == GEMINI_MODEL));
    // No TTS / Live / realtime audio, image, transcription, embedding,
    // video, music, computer-use, or agent ids may enter the curated
    // fallback: they reject Kivo's text request shape or return non-text
    // output. The dynamic ListModels path applies the same blocklist
    // (see filter_api_models below) with no allowlist of ids.
    for model in &models {
        assert!(!model.id.contains("tts"), "tts model listed: {}", model.id);
        assert!(
            !model.id.contains("live"),
            "live model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("audio"),
            "audio model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("image"),
            "image model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("banana"),
            "image model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("transcribe"),
            "audio model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("embed"),
            "embedding model listed: {}",
            model.id
        );
        assert!(
            !model.id.starts_with("veo"),
            "video model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("omni"),
            "video model listed: {}",
            model.id
        );
        assert!(
            !model.id.starts_with("lyria"),
            "music model listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("computer-use"),
            "agent listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("deep-research"),
            "agent listed: {}",
            model.id
        );
        assert!(
            !model.id.contains("robotics"),
            "specialized model listed: {}",
            model.id
        );
    }
    for excluded in [
        "gemini-2.5-flash-preview-tts",
        "gemini-2.5-pro-preview-tts",
        "gemini-2.5-flash-image",
        "gemini-3-pro-image",
        "gemini-2.5-flash-live",
        "gemini-2.5-flash-native-audio-preview-12-2025",
        "gemini-2.5-computer-use-preview-10-2025",
        "gemini-omni-1.1-flash",
        "gemini-embedding-001",
        "veo-3.1-preview",
        "lyria-3-pro-preview",
        "deep-research-pro-preview-12-2025",
    ] {
        assert!(!is_usable_model(excluded), "{excluded} must not be offered");
    }
}

#[test]
fn unknown_models_normalize_to_the_default() {
    assert_eq!(normalize_model("gemini-2.5-flash"), "gemini-2.5-flash");
    assert_eq!(normalize_model("  gemini-2.5-pro  "), "gemini-2.5-pro");
    assert_eq!(normalize_model(""), DEFAULT_GEMINI_MODEL);
    assert_eq!(
        normalize_model("gemini-2.5-flash-preview-tts"),
        DEFAULT_GEMINI_MODEL
    );
    assert_eq!(normalize_model("has spaces!"), DEFAULT_GEMINI_MODEL);
}

#[test]
fn blocklist_allows_new_models_and_strips_models_prefix() {
    // Newest text models keep working without a Kivo update,
    // including non-`gemini-` families.
    assert!(is_usable_model("gemini-4.0-flash"));
    assert!(is_usable_model("gemini-3.9-pro"));
    assert!(is_usable_model("gemma-3-27b-it"));
    assert!(is_usable_model("learnlm-2.0-flash"));
    assert_eq!(normalize_model("gemini-4.0-flash"), "gemini-4.0-flash");
    assert_eq!(normalize_model("gemma-3-27b-it"), "gemma-3-27b-it");
    // ListModels-style ids are canonicalized.
    assert_eq!(
        normalize_model("models/gemini-2.5-flash"),
        "gemini-2.5-flash"
    );
    assert!(is_usable_model("models/gemini-4.0-flash"));
    assert!(is_usable_model("models/gemma-3-27b-it"));
    // Blocked non-text families stay rejected.
    for blocked in [
        "gemini-2.5-flash-preview-tts",
        "gemini-2.5-flash-live",
        "gemini-2.5-flash-native-audio-preview-12-2025",
        "gemini-2.5-flash-image",
        "gemini-3-pro-image",
        "gemini-embedding-001",
        "gemini-3.5-transcribe",
        "gemini-2.5-computer-use-preview-10-2025",
        "gemini-omni-1.1-flash",
        "veo-3.1-preview",
        "lyria-3-pro-preview",
        "deep-research-pro-preview-12-2025",
        "gemini-robotics-er-2-preview",
        "nano-banana-pro-preview",
    ] {
        assert!(
            is_blocked_model(blocked) || !is_usable_model(blocked),
            "{blocked} must be rejected"
        );
        assert!(!is_usable_model(blocked), "{blocked} must not be offered");
    }
    // Malformed ids are rejected too.
    for malformed in [
        "",
        "ab",
        "GEMINI-2.5-FLASH",
        "gemini",
        "has spaces",
        "invalid!!",
    ] {
        assert!(!is_usable_model(malformed), "{malformed} must be rejected");
    }
}

#[test]
fn error_codes_match_the_real_interactions_api_shape() {
    // Live failures arrive array-wrapped with a numeric code, a status
    // string, and details[].reason — e.g. an invalid key is HTTP 400
    // INVALID_ARGUMENT / API_KEY_INVALID, not 401.
    let invalid_key = serde_json::json!([{
        "error": {
            "code": 400,
            "message": "API key not valid. Please pass a valid API key.",
            "status": "INVALID_ARGUMENT",
            "details": [{
                "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                "reason": "API_KEY_INVALID",
                "domain": "googleapis.com",
            }]
        }
    }]);
    assert_eq!(
        parse_api_error_code(&invalid_key).as_deref(),
        Some("authentication")
    );
    let error = GeminiError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        code: parse_api_error_code(&invalid_key),
        detail: parse_api_error_detail(&invalid_key),
    };
    assert_eq!(error.code(), "invalid_api_key");
    assert_eq!(
        error.user_message(),
        "Couldn't connect to Gemini. Check your API key."
    );

    // Legacy object shape with a string code keeps working.
    let legacy = serde_json::json!({"error": {"code": "permission_denied"}});
    assert_eq!(
        parse_api_error_code(&legacy).as_deref(),
        Some("permission_denied")
    );

    // Numeric codes without a status string survive as strings.
    let numeric = serde_json::json!({"error": {"code": 503}});
    assert_eq!(parse_api_error_code(&numeric).as_deref(), Some("503"));

    // Rate limits stay rate-limited.
    let exhausted = serde_json::json!({
        "error": {"code": 429, "status": "RESOURCE_EXHAUSTED", "message": "Slow down."}
    });
    let error = GeminiError::Api {
        status: reqwest::StatusCode::TOO_MANY_REQUESTS,
        code: parse_api_error_code(&exhausted),
        detail: parse_api_error_detail(&exhausted),
    };
    assert_eq!(error.code(), "rate_limited");
}

#[test]
fn error_codes_distinguish_unavailable_models_and_body_only_rate_limits() {
    // Unknown / retired model ids arrive as 404 NOT_FOUND on both the
    // Models GET and the Interactions POST. They must not read as key
    // problems: Test connection relies on this to advise picking another
    // model instead of re-entering the key.
    let not_found = serde_json::json!({
        "error": {"code": 404, "status": "NOT_FOUND", "message": "Model not found."}
    });
    assert_eq!(
        parse_api_error_code(&not_found).as_deref(),
        Some("not_found")
    );
    let error = GeminiError::Api {
        status: reqwest::StatusCode::NOT_FOUND,
        code: parse_api_error_code(&not_found),
        detail: parse_api_error_detail(&not_found),
    };
    assert!(error.is_not_found());
    assert!(!error.is_rate_limited());
    assert_eq!(error.code(), "model_not_found");
    // Server message wins when present; tailored guidance without one.
    assert_eq!(error.user_message(), "Model not found.");
    let bare = GeminiError::Api {
        status: reqwest::StatusCode::NOT_FOUND,
        code: Some("not_found".into()),
        detail: None,
    };
    assert!(bare.user_message().contains("isn't available"));

    // Quota exhaustion is not always HTTP 429: a 400/403 carrying
    // RESOURCE_EXHAUSTED must still trigger the backup retry.
    let exhausted_body = serde_json::json!({
        "error": {"code": 400, "status": "RESOURCE_EXHAUSTED", "message": "Quota."}
    });
    let error = GeminiError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        code: parse_api_error_code(&exhausted_body),
        detail: parse_api_error_detail(&exhausted_body),
    };
    assert!(error.is_rate_limited());
    assert!(!error.is_not_found());
    assert_eq!(error.code(), "rate_limited");
}

#[test]
fn quota_errors_allow_failover_even_with_forbidden_status() {
    for status in [
        reqwest::StatusCode::BAD_REQUEST,
        reqwest::StatusCode::FORBIDDEN,
        reqwest::StatusCode::TOO_MANY_REQUESTS,
    ] {
        let body = serde_json::json!({
            "error": {"status": "RESOURCE_EXHAUSTED", "message": "Quota exceeded."}
        });
        let error = GeminiError::Api {
            status,
            code: parse_api_error_code(&body),
            detail: parse_api_error_detail(&body),
        };
        assert_eq!(error.code(), "rate_limited");
        assert!(!error.is_failover_terminal());
        assert_eq!(
            error.user_message(),
            "Gemini is temporarily rate limited. Try again shortly."
        );
    }
    let forbidden = GeminiError::Api {
        status: reqwest::StatusCode::FORBIDDEN,
        code: Some("permission_denied".into()),
        detail: None,
    };
    assert!(forbidden.is_failover_terminal());
    assert_eq!(forbidden.code(), "invalid_api_key");
}

#[test]
fn server_error_detail_surfaces_verbatim_and_truncates() {
    // Real invalid-thinking-level shape: no status string, string code.
    let bad_level = serde_json::json!({
        "error": {
            "message": "'low' is not a supported thinking level for this model. Allowed values are: high, minimal.",
            "code": "invalid_request"
        }
    });
    assert_eq!(
        parse_api_error_detail(&bad_level).as_deref(),
        Some(
            "'low' is not a supported thinking level for this model. Allowed values are: high, minimal."
        )
    );
    let error = GeminiError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        code: parse_api_error_code(&bad_level),
        detail: parse_api_error_detail(&bad_level),
    };
    assert!(error.user_message().contains("thinking level"));
    // No message anywhere -> generic fallback.
    let bare = GeminiError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        code: None,
        detail: None,
    };
    assert_eq!(
        bare.user_message(),
        "Gemini couldn't complete that request."
    );
    // Long messages (doc URLs, model lists) cap at 300 chars.
    let long = serde_json::json!({"error": {"message": "x".repeat(500)}});
    assert_eq!(parse_api_error_detail(&long).unwrap().chars().count(), 300);
    assert!(parse_api_error_detail(&serde_json::json!({"error": {}})).is_none());
}

#[test]
fn server_error_detail_yields_retry_message() {
    // Live Gemma shape: HTTP 500 with a content-free message.
    let internal = serde_json::json!({
        "error": {"message": "Internal error encountered.", "code": "api_error"}
    });
    let error = GeminiError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        code: parse_api_error_code(&internal),
        detail: parse_api_error_detail(&internal),
    };
    assert!(error.is_server_error());
    assert_eq!(
        error.user_message(),
        "Gemini hit a temporary error. Try again shortly."
    );
    assert_eq!(error.code(), "api_error");
}

fn api_model(name: &str, display_name: Option<&str>, description: Option<&str>) -> ApiModel {
    ApiModel {
        name: name.to_owned(),
        display_name: display_name.map(str::to_owned),
        description: description.map(str::to_owned),
    }
}

#[test]
fn dynamic_list_filters_by_blocklist_only_and_sorts_default_first() {
    let models = vec![
        api_model(
            "models/gemini-2.5-flash",
            Some("Gemini 2.5 Flash"),
            Some("Fast."),
        ),
        api_model(
            "models/gemini-2.5-flash-preview-tts",
            Some("Gemini 2.5 Flash TTS"),
            Some("Speech."),
        ),
        api_model(
            "models/gemini-2.5-flash-native-audio-preview-12-2025",
            Some("Audio"),
            None,
        ),
        api_model(
            "models/gemini-2.5-computer-use-preview-10-2025",
            Some("Computer Use"),
            None,
        ),
        api_model("models/gemini-omni-1.1-flash", Some("Omni"), None),
        api_model("models/gemini-4.0-flash", None, None),
        api_model("models/gemini-4.0-flash", None, None),
        api_model("models/gemini-embedding-001", Some("Embedding"), None),
        api_model(
            "gemini-3.8-flash",
            Some("Gemini 3.8 Flash"),
            Some("Default."),
        ),
        api_model("gemma-3-27b-it", Some("Gemma 3 27B"), None),
        api_model("has spaces!", Some("Other"), None),
    ];
    let listed = filter_api_models(models);
    let ids: Vec<&str> = listed.iter().map(|model| model.id.as_str()).collect();
    // Default first, then alphabetical by label; blocked families gone;
    // newest text ids survive with no allowlist update.
    assert_eq!(ids[0], "gemini-3.8-flash");
    assert!(ids.contains(&"gemini-2.5-flash"));
    assert!(ids.contains(&"gemini-4.0-flash"));
    assert!(ids.contains(&"gemma-3-27b-it"));
    for blocked in [
        "gemini-2.5-flash-preview-tts",
        "gemini-2.5-flash-native-audio-preview-12-2025",
        "gemini-2.5-computer-use-preview-10-2025",
        "gemini-omni-1.1-flash",
        "gemini-embedding-001",
        "has spaces!",
    ] {
        assert!(!ids.contains(&blocked), "{blocked} must be filtered");
    }
    assert_eq!(listed.len(), ids.len());
    let flash = listed
        .iter()
        .find(|model| model.id == "gemini-2.5-flash")
        .unwrap();
    assert_eq!(flash.label, "Gemini 2.5 Flash");
    let newest = listed
        .iter()
        .find(|model| model.id == "gemini-4.0-flash")
        .unwrap();
    assert_eq!(newest.label, "gemini-4.0-flash");
}

#[test]
fn curated_fallback_never_empty_and_matches_supported() {
    let curated = curated_listed_models();
    let supported = supported_models();
    assert!(!curated.is_empty());
    assert_eq!(curated.len(), supported.len());
    assert!(curated.iter().any(|model| model.id == DEFAULT_GEMINI_MODEL));
}

/// Minimal GET fixture: accepts `expected` connections, records the
/// request path + `x-goog-api-key` header, and replays canned responses.
/// Separate from the POST Interactions fixtures in `commands/tests.rs`,
/// which require a JSON body that GET metadata calls never send.
struct GetFixture {
    models_endpoint: String,
    observed: std::sync::mpsc::Receiver<(String, Option<String>)>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl GetFixture {
    fn new(responses: Vec<(u16, &'static str)>) -> Self {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::mpsc,
            time::Duration,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let models_endpoint = format!("http://{}/models", listener.local_addr().unwrap());
        let (observed_tx, observed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let Ok(read) = stream.read(&mut buffer) else {
                        break;
                    };
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    if bytes.windows(4).any(|value| value == b"\r\n\r\n") {
                        break;
                    }
                    if bytes.len() > 16_384 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let path = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                let key = text.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("x-goog-api-key")
                        .then(|| value.trim().to_owned())
                });
                let _ = observed_tx.send((path, key));
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            models_endpoint,
            observed,
            worker: Some(worker),
        }
    }

    fn next_request(&self) -> (String, Option<String>) {
        self.observed
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("expected another GET request")
    }
}

impl Drop for GetFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn test_secret() -> crate::security::SecretString {
    crate::security::SecretString::new("test-key".into()).unwrap()
}

#[tokio::test]
async fn list_models_maps_invalid_key_to_auth_error() {
    let body = r#"[{"error":{"code":400,"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}]"#;
    let fixture = GetFixture::new(vec![(400, body)]);
    let client = super::GeminiClient::with_endpoints(
        "http://127.0.0.1:9/interactions".into(),
        fixture.models_endpoint.clone(),
    )
    .unwrap();
    let error = client.list_models(&test_secret()).await.unwrap_err();
    assert_eq!(error.code(), "invalid_api_key");
    assert_eq!(
        error.user_message(),
        "Couldn't connect to Gemini. Check your API key."
    );
}

#[tokio::test]
async fn list_models_paginates_and_filters_blocked_families() {
    let page_one = r#"{"models":[
      {"name":"models/gemini-3.8-flash","displayName":"Gemini 3.8 Flash","description":"Default."},
      {"name":"models/gemini-2.5-flash-preview-tts","displayName":"TTS"}
    ],"nextPageToken":"second"}"#;
    let page_two = r#"{"models":[
      {"name":"models/gemini-4.0-flash","displayName":"Gemini 4.0 Flash"}
    ]}"#;
    let fixture = GetFixture::new(vec![(200, page_one), (200, page_two)]);
    let client = super::GeminiClient::with_endpoints(
        "http://127.0.0.1:9/interactions".into(),
        fixture.models_endpoint.clone(),
    )
    .unwrap();
    let listed = client.list_models(&test_secret()).await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, vec!["gemini-3.8-flash", "gemini-4.0-flash"]);
    // Both pages requested; key sent via header, never in the URL.
    let (first_path, first_key) = fixture.next_request();
    let (second_path, _) = fixture.next_request();
    assert!(first_path.contains("/models"), "path was {first_path}");
    assert!(
        first_path.contains("pageSize=1000"),
        "path was {first_path}"
    );
    assert!(!first_path.contains("test-key"), "key leaked into URL");
    assert_eq!(first_key.as_deref(), Some("test-key"));
    assert!(
        second_path.contains("pageToken=second"),
        "path was {second_path}"
    );
}
