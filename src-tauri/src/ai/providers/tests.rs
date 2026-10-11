use super::*;

#[test]
fn parses_usage_for_openai_messages_responses_and_google_shapes() {
    let cases = [
        (
            CompletionProtocol::Chat,
            serde_json::json!({"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":12,"completion_tokens":4}}),
            Some(12),
            Some(4),
        ),
        (
            CompletionProtocol::Messages,
            serde_json::json!({"content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":13,"output_tokens":5}}),
            Some(13),
            Some(5),
        ),
        (
            CompletionProtocol::Google,
            serde_json::json!({"candidates":[{"content":{"parts":[{"text":"ok"}]}}],"usageMetadata":{"promptTokenCount":14,"candidatesTokenCount":6}}),
            Some(14),
            Some(6),
        ),
    ];
    for (protocol, body, expected_in, expected_out) in cases {
        let (_, tokens_in, tokens_out) = parse_completion_with_usage(protocol, body).unwrap();
        assert_eq!(tokens_in, expected_in);
        assert_eq!(tokens_out, expected_out);
    }
}

#[tokio::test]
async fn go_probe_omits_optional_tuning_but_generation_keeps_it() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/chat/completions", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut sessions = Vec::new();
        for request_index in 0..3 {
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
            assert!(headers.contains("x-opencode-client: kivo"));
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
            assert!(body.get("temperature").is_none());
            match request_index {
                0 => {
                    assert_eq!(body["model"], "glm-5.3-flash");
                    assert!(body.get("reasoning_effort").is_none());
                    let response = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
                }
                1 => {
                    assert_eq!(body["model"], "glm-5.3-flash");
                    assert_eq!(body["reasoning_effort"], "low");
                    let response =
                        r#"{"error":{"code":"rate_limit_exceeded","message":"try fallback"}}"#;
                    write!(stream, "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
                }
                _ => {
                    assert_eq!(body["model"], "glm-5.3");
                    assert_eq!(body["reasoning_effort"], "low");
                    let response = r#"{"choices":[{"message":{"content":"hello world"}}]}"#;
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
                }
            }
        }
        assert_ne!(sessions[0], sessions[1]);
        assert_eq!(sessions[1], sessions[2]);
    });
    let client = OpenAiCompatClient::new().unwrap();
    let key = SecretString::new("test-key".to_owned()).unwrap();
    client
        .test_connection(AiProvider::Go, &url, Some(&key), "glm-5.3-flash")
        .await
        .unwrap();
    let prompt = AiPrompt {
        system_instruction: "Correct spelling".into(),
        input: "helo world".into(),
    };
    assert_eq!(
        client
            .generate_in_order_with_model(
                AiProvider::Go,
                &url,
                Some(&key),
                &["glm-5.3-flash".into(), "glm-5.3".into()],
                &prompt,
                AiReasoningMode::Fast,
            )
            .await
            .unwrap(),
        ("hello world".to_owned(), "glm-5.3".to_owned(), None, None)
    );
    server.join().unwrap();
    let custom = client
        .chat_request(AiProvider::Custom, &url, None)
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
    let auth = serde_json::json!({"error": {"type": "AuthError", "message": "Invalid API key."}});
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
    let empty = serde_json::json!({"error": {"message": "Insufficient balance, please top up."}});
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
        AiProvider::Go,
        AiReasoningMode::Fast,
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
        max_tokens: Some(16),
        ..request
    };
    let (_, body) = completion_request(
        CompletionProtocol::Messages,
        AiProvider::Go,
        AiReasoningMode::Fast,
        "https://opencode.ai/zen/go/v1/chat/completions",
        &probe,
    );
    assert_eq!(body["max_tokens"], 16);
}

#[test]
fn go_account_and_forbidden_errors_are_not_misreported_as_bad_keys() {
    let region = serde_json::json!({
        "error": {
            "code": "Account.RegionUnavailable",
            "message": "This Go model requires Global regions."
        }
    });
    let region_error = OpencodeError::Api {
        status: StatusCode::FORBIDDEN,
        code: parse_openai_error_code(&region),
    };
    assert_eq!(region_error.code(), "region_unavailable");
    assert!(region_error.user_message().contains("Global regions"));
    assert!(!is_failover_terminal(&region_error));

    let disabled = serde_json::json!({
        "error": {
            "code": "Account.Disabled",
            "message": "An active OpenCode Go subscription is required to use Go models."
        }
    });
    let disabled_error = OpencodeError::Api {
        status: StatusCode::FORBIDDEN,
        code: parse_openai_error_code(&disabled),
    };
    assert_eq!(disabled_error.code(), "account_disabled");
    assert!(
        disabled_error
            .user_message()
            .contains("active Go subscription")
    );
    assert!(is_failover_terminal(&disabled_error));

    let forbidden = serde_json::json!({
        "error": {
            "message": "Forbidden: {\"model\":\"glm-5.3-flash\"}"
        }
    });
    let forbidden_error = OpencodeError::Api {
        status: StatusCode::FORBIDDEN,
        code: parse_openai_error_code(&forbidden),
    };
    assert_eq!(forbidden_error.code(), "provider_forbidden");
    assert!(!is_failover_terminal(&forbidden_error));

    let auth = serde_json::json!({
        "error": {
            "type": "AuthError",
            "message": "Invalid API key."
        }
    });
    let auth_error = OpencodeError::Api {
        status: StatusCode::FORBIDDEN,
        code: parse_openai_error_code(&auth),
    };
    assert_eq!(auth_error.code(), "invalid_api_key");
    assert!(is_failover_terminal(&auth_error));
}

#[test]
fn reasoning_presets_use_only_documented_hosted_controls() {
    let messages = [ChatMessage {
        role: "user",
        content: "helo world",
    }];
    let request = ChatCompletionRequest {
        model: "glm-5.3-flash",
        messages: &messages,
        max_tokens: None,
    };
    for (mode, expected) in [
        (AiReasoningMode::Fast, "low"),
        (AiReasoningMode::Balanced, "high"),
        (AiReasoningMode::Deep, "max"),
    ] {
        let (_, body) = completion_request(
            CompletionProtocol::Chat,
            AiProvider::Go,
            mode,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &request,
        );
        assert_eq!(body["reasoning_effort"], expected);
    }

    for (model, expected) in [
        ("glm-5.2", [None, Some("high"), Some("max")]),
        ("deepseek-v4-pro", [None, Some("high"), Some("max")]),
    ] {
        let request = ChatCompletionRequest {
            model,
            messages: &messages,
            max_tokens: None,
        };
        for (mode, expected) in [
            (AiReasoningMode::Fast, expected[0]),
            (AiReasoningMode::Balanced, expected[1]),
            (AiReasoningMode::Deep, expected[2]),
        ] {
            let (_, body) = completion_request(
                CompletionProtocol::Chat,
                AiProvider::Go,
                mode,
                "https://opencode.ai/zen/go/v1/chat/completions",
                &request,
            );
            assert_eq!(body["reasoning_effort"].as_str(), expected);
        }
    }

    let request = ChatCompletionRequest {
        model: "qwen3.8-flash",
        messages: &messages,
        max_tokens: None,
    };
    let (_, body) = completion_request(
        CompletionProtocol::Messages,
        AiProvider::Go,
        AiReasoningMode::Fast,
        "https://opencode.ai/zen/go/v1/chat/completions",
        &request,
    );
    assert_eq!(body["thinking"]["type"], "disabled");
    assert!(body.get("output_config").is_none());
    let (_, body) = completion_request(
        CompletionProtocol::Messages,
        AiProvider::Go,
        AiReasoningMode::Deep,
        "https://opencode.ai/zen/go/v1/chat/completions",
        &request,
    );
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["output_config"]["effort"], "xhigh");

    for model in ["qwen3.8-max", "qwen3.9-flash"] {
        let request = ChatCompletionRequest {
            model,
            messages: &messages,
            max_tokens: None,
        };
        let (_, body) = completion_request(
            CompletionProtocol::Messages,
            AiProvider::Go,
            AiReasoningMode::Balanced,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &request,
        );
        assert!(body.get("reasoning_effort").is_none(), "{model}");
        assert!(body.get("thinking").is_none(), "{model}");
    }

    for model in ["glm-5.4-flash", "deepseek-v5-flash"] {
        let request = ChatCompletionRequest {
            model,
            messages: &messages,
            max_tokens: None,
        };
        let (_, body) = completion_request(
            CompletionProtocol::Chat,
            AiProvider::Go,
            AiReasoningMode::Balanced,
            "https://opencode.ai/zen/go/v1/chat/completions",
            &request,
        );
        assert!(body.get("reasoning_effort").is_none(), "{model}");
    }

    let request = ChatCompletionRequest {
        model: "kimi-k2.7-code",
        messages: &messages,
        max_tokens: None,
    };
    let (_, body) = completion_request(
        CompletionProtocol::Chat,
        AiProvider::Go,
        AiReasoningMode::Deep,
        "https://opencode.ai/zen/go/v1/chat/completions",
        &request,
    );
    assert!(body.get("reasoning_effort").is_none());
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
        AiProvider::Go,
        AiReasoningMode::Fast,
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
        AiProvider::Go,
        AiReasoningMode::Fast,
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
        AiProvider::Zen,
        AiReasoningMode::Fast,
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
    assert_eq!(error.code(), "provider_rejected");
    assert!(!is_failover_terminal(&error));
    assert!(error.user_message().contains("HTTP 400"));
    assert!(
        error
            .user_message()
            .contains("does not support the requested temperature")
    );
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

    let console_go = serde_json::json!({
        "error": {
            "type": "server_error",
            "message": "Error from provider (Console Go): Upstream request failed: [1210] API parameter invalid, please check documentation."
        }
    });
    let console_go_error = OpencodeError::Api {
        status: StatusCode::BAD_REQUEST,
        code: parse_openai_error_code(&console_go),
    };
    assert_eq!(console_go_error.code(), "provider_rejected");
    assert!(console_go_error.user_message().contains("[1210]"));

    let plain = provider_message_code("gateway exploded").unwrap();
    let plain_error = OpencodeError::Api {
        status: StatusCode::BAD_REQUEST,
        code: Some(plain),
    };
    assert_eq!(plain_error.code(), "provider_rejected");
    assert!(plain_error.user_message().contains("gateway exploded"));
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
