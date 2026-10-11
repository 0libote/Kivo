use std::{env, fs, time::SystemTime};

use super::{
    AppSettings, HostPlatform, SettingsRepository, ShortcutBinding, SpeechEnginePreference,
    ThemePreference, dictation_default_for, foreign_default_replacement, writing_tools_default_for,
};

fn temporary_settings_path() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("kivo-settings-{unique}.json"))
}

#[test]
fn missing_settings_use_defaults() {
    let path = temporary_settings_path();
    let repository = SettingsRepository::new(&path);
    assert_eq!(repository.load().unwrap(), AppSettings::default());
}

#[test]
fn settings_round_trip_without_a_secret_field() {
    let path = temporary_settings_path();
    let repository = SettingsRepository::new(&path);
    let mut settings = AppSettings::default();
    settings.general.theme = ThemePreference::Dark;
    settings.writing_tools.enabled_actions = vec!["proofread".to_owned()];

    repository.save(&settings).unwrap();
    assert_eq!(repository.load().unwrap(), settings);

    settings.general.theme = ThemePreference::Light;
    repository.save(&settings).unwrap();
    assert_eq!(repository.load().unwrap(), settings);
    let persisted = fs::read_to_string(&path).unwrap();
    assert!(!persisted.to_ascii_lowercase().contains("api_key"));
    assert!(!persisted.to_ascii_lowercase().contains("apikey"));
    let _ = fs::remove_file(path);
}

#[test]
fn voz_engine_preference_round_trips_as_an_independent_choice() {
    let path = temporary_settings_path();
    let repository = SettingsRepository::new(&path);
    let mut settings = AppSettings::default();
    settings.dictation.speech_engine = SpeechEnginePreference::Voz;
    repository.save(&settings).unwrap();
    assert_eq!(
        repository.load().unwrap().dictation.speech_engine,
        SpeechEnginePreference::Voz
    );
    let _ = fs::remove_file(path);
}

#[test]
fn foreign_native_dictation_shortcut_migrates_to_the_host_default() {
    // Host-parameterized: every CI platform executes both migration
    // directions, so a default changed on one OS without its counterpart
    // fails fast instead of surfacing weeks later on the other OS.
    // Windows host: macOS "Fn" migrates; everything else passes through.
    assert_eq!(
        foreign_default_replacement("Fn", HostPlatform::Windows),
        Some(ShortcutBinding::new("Ctrl+Meta"))
    );
    assert_eq!(
        foreign_default_replacement("Ctrl+Alt+D", HostPlatform::Windows),
        None
    );
    assert_eq!(
        foreign_default_replacement("Ctrl+Meta", HostPlatform::Windows),
        None
    );
    // Linux host: both native defaults migrate to the portable default.
    assert_eq!(
        foreign_default_replacement("Fn", HostPlatform::Linux),
        Some(ShortcutBinding::new("Control+Alt+Space"))
    );
    assert_eq!(
        foreign_default_replacement("Ctrl+Meta", HostPlatform::Linux),
        Some(ShortcutBinding::new("Control+Alt+Space"))
    );
    assert_eq!(
        foreign_default_replacement("Control+Super", HostPlatform::Linux),
        Some(ShortcutBinding::new("Control+Alt+Space"))
    );
    assert_eq!(
        foreign_default_replacement("Ctrl+Alt+D", HostPlatform::Linux),
        None
    );

    // Legacy macOS settings must not retain an unusable Fn binding.
    let mut legacy = AppSettings::default();
    legacy.dictation.shortcut = ShortcutBinding::new("Fn");
    let migrated = legacy.validate_and_normalize().unwrap();
    assert_eq!(
        migrated.dictation.shortcut,
        dictation_default_for(HostPlatform::current())
    );

    // End-to-end through normalization on the current host: the host's
    // own default survives while custom shortcuts pass through untouched.
    let mut settings = AppSettings::default();
    settings.dictation.shortcut = ShortcutBinding::new("Ctrl+Alt+D");
    let migrated = settings.validate_and_normalize().unwrap();
    assert_eq!(
        migrated.dictation.shortcut,
        ShortcutBinding::new("Ctrl+Alt+D")
    );
}

#[test]
fn platform_defaults_match_the_frontend_contract() {
    // Mirror of src/types.ts defaultSettings() and the frontend
    // platform-defaults test. If either side changes a default, update
    // both together (and scripts/check-contracts.ts enforces it in
    // CI without compiling).
    assert_eq!(
        dictation_default_for(HostPlatform::Windows),
        ShortcutBinding::new("Ctrl+Meta")
    );
    assert_eq!(
        dictation_default_for(HostPlatform::Linux),
        ShortcutBinding::new("Control+Alt+Space")
    );
    assert_eq!(
        writing_tools_default_for(HostPlatform::Windows),
        ShortcutBinding::new("Ctrl+Space")
    );
    assert_eq!(
        writing_tools_default_for(HostPlatform::Linux),
        ShortcutBinding::new("Ctrl+Space")
    );
    // Linux dictation and writing defaults must differ: sharing one
    // accelerator would register the same global shortcut twice.
    assert_ne!(
        dictation_default_for(HostPlatform::Linux),
        writing_tools_default_for(HostPlatform::Linux)
    );
}

#[test]
fn duplicate_actions_are_normalized() {
    let mut settings = AppSettings::default();
    settings.writing_tools.enabled_actions = vec![
        "proofread".to_owned(),
        "proofread".to_owned(),
        "concise".to_owned(),
        // Legacy enum camelCase spelling folds onto the wire id.
        "keyPoints".to_owned(),
    ];

    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(
        settings.writing_tools.enabled_actions,
        vec![
            "proofread".to_owned(),
            "concise".to_owned(),
            "key-points".to_owned()
        ]
    );
}

#[test]
fn writing_presets_normalize_and_pin_models_per_provider() {
    use super::WritingPresetSettings;
    let mut settings = AppSettings::default();
    settings.writing_tools.presets = vec![
        WritingPresetSettings {
            id: "  custom-1 ".into(),
            label: "  Pirate  ".into(),
            instruction: "Rewrite like a pirate.".into(),
            replaces_selection: true,
            models: vec!["  gemini-2.5-flash  ".into(), "gemini-2.5-flash".into()],
            ..WritingPresetSettings::default()
        },
        // Duplicate id: the first occurrence wins.
        WritingPresetSettings {
            id: "custom-1".into(),
            ..WritingPresetSettings::default()
        },
    ];
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.writing_tools.presets.len(), 1);
    let preset = &settings.writing_tools.presets[0];
    assert_eq!(preset.id, "custom-1");
    assert_eq!(preset.label, "Pirate");
    assert_eq!(preset.models, vec!["gemini-2.5-flash".to_owned()]);
}

#[test]
fn ai_model_defaults_and_legacy_files() {
    // Default matches the frontend contract (src/types.ts DEFAULT_AI_MODEL).
    assert_eq!(
        AppSettings::default().ai.models,
        vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
    );

    // Supported ids survive normalization (trimmed), including new ids
    // that were never in the curated suggestion list.
    let mut settings = AppSettings::default();
    settings.ai.models = vec![
        "  gemini-2.5-flash  ".into(),
        "models/gemini-4.0-flash".into(),
    ];
    assert_eq!(
        settings.validate_and_normalize().unwrap().ai.models,
        vec!["gemini-2.5-flash", "gemini-4.0-flash"]
    );

    // Duplicates collapse (first wins) and unknown / TTS / image ids are
    // dropped; an emptied queue falls back to the default instead of
    // bricking AI requests.
    let mut settings = AppSettings::default();
    settings.ai.models = vec![
        "gemini-2.5-flash".into(),
        "gemini-2.5-flash".into(),
        "has spaces!".into(),
        "gemini-2.5-flash-preview-tts".into(),
    ];
    assert_eq!(
        settings.validate_and_normalize().unwrap().ai.models,
        vec!["gemini-2.5-flash".to_owned()]
    );
    let mut settings = AppSettings::default();
    settings.ai.models = vec![];
    assert_eq!(
        settings.validate_and_normalize().unwrap().ai.models,
        vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
    );

    // Files written before the ai section existed deserialize via serde
    // defaults and keep working.
    let legacy = serde_json::json!({
        "schemaVersion": 1,
        "general": {},
        "dictation": { "shortcut": { "accelerator": "Fn" } },
        "writingTools": {
            "shortcut": { "accelerator": "Ctrl+Shift+Space" },
            "enabledActions": ["proofread"],
        },
    });
    let settings: AppSettings = serde_json::from_value(legacy).unwrap();
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(
        settings.ai.models,
        vec![crate::ai::DEFAULT_GEMINI_MODEL.to_owned()]
    );
}

#[test]
fn go_default_upgrade_prefers_the_fast_model_without_overwriting_new_choices() {
    let old = AppSettings {
        schema_version: 1,
        ai: super::AiSettings {
            provider: crate::ai::AiProvider::Go,
            models: vec!["kimi-k2.7-code".into()],
            ..super::AiSettings::default()
        },
        ..AppSettings::default()
    };
    let old = old.validate_and_normalize().unwrap();
    assert_eq!(old.schema_version, super::SETTINGS_SCHEMA_VERSION);
    assert_eq!(old.ai.models, vec!["glm-5.3-flash"]);

    let mut explicit = AppSettings::default();
    explicit.ai.provider = crate::ai::AiProvider::Go;
    explicit.ai.models = vec!["kimi-k2.7-code".into()];
    let explicit = explicit.validate_and_normalize().unwrap();
    assert_eq!(explicit.ai.models, vec!["kimi-k2.7-code"]);
}

#[test]
fn ai_legacy_primary_and_backup_migrate_into_the_queue() {
    // Pre-queue files stored `model` + `backupModel`: both migrate in
    // order, and the legacy keys are never written back out.
    let legacy = serde_json::json!({
        "schemaVersion": 1,
        "general": {},
        "dictation": { "shortcut": { "accelerator": "Fn" } },
        "writingTools": {
            "shortcut": { "accelerator": "Ctrl+Shift+Space" },
            "enabledActions": ["proofread"],
        },
        "ai": {
            "provider": "zen",
            "model": "opencode/kimi-k2.7-code",
            "backupModel": "glm-5.3-flash",
        },
    });
    let settings: AppSettings = serde_json::from_value(legacy).unwrap();
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.models, vec!["kimi-k2.7-code", "glm-5.3-flash"]);
    let saved = serde_json::to_value(&settings).unwrap();
    assert!(saved["ai"].get("models").is_some());
    assert!(saved["ai"].get("model").is_none());
    assert!(saved["ai"].get("backupModel").is_none());

    // An explicit queue always wins over stale legacy fields.
    let mixed = serde_json::json!({
        "schemaVersion": 1,
        "ai": {
            "models": ["glm-5.3-flash"],
            "model": "gemini-2.5-flash",
            "backupModel": "gemini-2.5-flash-lite",
        },
    });
    let settings: AppSettings = serde_json::from_value(mixed).unwrap();
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.models, vec!["glm-5.3-flash".to_owned()]);
}

#[test]
fn ai_queues_truncate_to_the_cap() {
    let mut settings = AppSettings::default();
    settings.ai.models = (0..crate::ai::MAX_AI_MODELS + 3)
        .map(|n| format!("gemini-test-{n}-flash"))
        .collect();
    let models = settings.validate_and_normalize().unwrap().ai.models;
    assert_eq!(models.len(), crate::ai::MAX_AI_MODELS);
    assert_eq!(models[0], "gemini-test-0-flash");
}

#[test]
fn ai_provider_defaults_to_gemini_and_legacy_files_keep_working() {
    use crate::ai::AiProvider;
    assert_eq!(AppSettings::default().ai.provider, AiProvider::Gemini);
    assert_eq!(
        AppSettings::default().ai.reasoning_mode,
        crate::ai::AiReasoningMode::Fast
    );
    assert_eq!(AppSettings::default().ai.custom_base_url, None);
    // Files written before the provider field existed deserialize via
    // serde defaults to Gemini, preserving the stored Gemini key slot.
    let legacy = serde_json::json!({
        "schemaVersion": 1,
        "general": {},
        "dictation": { "shortcut": { "accelerator": "Fn" } },
        "writingTools": {
            "shortcut": { "accelerator": "Ctrl+Shift+Space" },
            "enabledActions": ["proofread"],
        },
        "ai": { "model": "gemini-2.5-flash" },
    });
    let settings: AppSettings = serde_json::from_value(legacy).unwrap();
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.provider, AiProvider::Gemini);
    assert_eq!(settings.ai.models, vec!["gemini-2.5-flash".to_owned()]);

    let explicit = serde_json::json!({"ai": {"reasoningMode": "deep"}});
    let settings: AppSettings = serde_json::from_value(explicit).unwrap();
    assert_eq!(
        settings.validate_and_normalize().unwrap().ai.reasoning_mode,
        crate::ai::AiReasoningMode::Deep
    );
}

#[test]
fn ai_provider_switch_falls_back_to_usable_models() {
    use crate::ai::AiProvider;
    // Zen keeps Gemini text ids (usable there) but drops TTS families.
    let mut settings = AppSettings::default();
    settings.ai.provider = AiProvider::Zen;
    settings.ai.models = vec!["gemini-2.5-flash".into()];
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.models, vec!["gemini-2.5-flash".to_owned()]);
    let mut settings = AppSettings::default();
    settings.ai.provider = AiProvider::Zen;
    settings.ai.models = vec!["gemini-2.5-flash-preview-tts".into()];
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(
        settings.ai.models,
        vec![AiProvider::Zen.default_model().to_owned()]
    );
    // Custom accepts local ids and normalizes the base URL.
    let mut settings = AppSettings::default();
    settings.ai.provider = AiProvider::Custom;
    settings.ai.models = vec!["  google/gemma-3n-e4b  ".into()];
    settings.ai.custom_base_url = Some("http://127.0.0.1:1234/v1/".into());
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.models, vec!["google/gemma-3n-e4b".to_owned()]);
    assert_eq!(
        settings.ai.custom_base_url.as_deref(),
        Some("http://127.0.0.1:1234/v1")
    );
    // Junk base URLs collapse to None (Ollama default applies).
    let mut settings = AppSettings::default();
    settings.ai.provider = AiProvider::Custom;
    settings.ai.custom_base_url = Some("notaurl".into());
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.ai.custom_base_url, None);
}

#[test]
fn dictation_cleanup_model_follows_the_provider() {
    // Unset by default: cleanup follows the Writing Tools queue.
    assert_eq!(AppSettings::default().dictation.cleanup_model, None);

    // A model usable on the current provider survives normalization.
    let mut settings = AppSettings::default();
    settings.dictation.cleanup_model = Some("gemini-2.5-flash-lite".into());
    assert_eq!(
        settings
            .validate_and_normalize()
            .unwrap()
            .dictation
            .cleanup_model
            .as_deref(),
        Some("gemini-2.5-flash-lite")
    );

    // A model only valid on another provider is dropped, so cleanup falls
    // back to the queue instead of failing on every dictation.
    let mut settings = AppSettings::default();
    settings.dictation.cleanup_model = Some("google/gemma-3n-e4b".into());
    assert_eq!(
        settings
            .validate_and_normalize()
            .unwrap()
            .dictation
            .cleanup_model,
        None
    );

    // Blank overrides normalize away.
    let mut settings = AppSettings::default();
    settings.dictation.cleanup_model = Some("   ".into());
    assert_eq!(
        settings
            .validate_and_normalize()
            .unwrap()
            .dictation
            .cleanup_model,
        None
    );
}

#[test]
fn dictation_vocabulary_normalizes_and_caps() {
    use super::VocabularyEntry;
    fn entry(word: &str, meaning: &str) -> VocabularyEntry {
        VocabularyEntry {
            word: word.into(),
            meaning: meaning.into(),
        }
    }
    // Empty by default.
    assert!(AppSettings::default().dictation.vocabulary.is_empty());

    // Trims, drops empties, folds case-insensitive duplicates (first
    // spelling wins, later duplicates still fill a missing meaning).
    let mut settings = AppSettings::default();
    settings.dictation.vocabulary = vec![
        entry("  Kivo  ", ""),
        entry("", "dropped"),
        entry("kivo", "our product"),
        entry("SOC 2", "compliance framework"),
        entry("soc 2 ", ""),
    ];
    let normalized = settings.validate_and_normalize().unwrap();
    assert_eq!(
        normalized.dictation.vocabulary,
        vec![
            entry("Kivo", "our product"),
            entry("SOC 2", "compliance framework"),
        ]
    );

    // Over-long words and meanings truncate to their caps instead of
    // failing.
    let mut settings = AppSettings::default();
    settings.dictation.vocabulary = vec![entry(
        &"x".repeat(super::MAX_VOCABULARY_WORD_CHARS + 10),
        &"y".repeat(super::MAX_VOCABULARY_MEANING_CHARS + 10),
    )];
    let normalized = settings.validate_and_normalize().unwrap();
    assert_eq!(normalized.dictation.vocabulary.len(), 1);
    assert_eq!(
        normalized.dictation.vocabulary[0].word.chars().count(),
        super::MAX_VOCABULARY_WORD_CHARS
    );
    assert_eq!(
        normalized.dictation.vocabulary[0].meaning.chars().count(),
        super::MAX_VOCABULARY_MEANING_CHARS
    );

    // Huge imports truncate to the cap (first entries win).
    let mut settings = AppSettings::default();
    settings.dictation.vocabulary = (0..super::MAX_VOCABULARY_WORDS + 50)
        .map(|n| entry(&format!("word-{n}"), ""))
        .collect();
    let normalized = settings.validate_and_normalize().unwrap();
    assert_eq!(
        normalized.dictation.vocabulary.len(),
        super::MAX_VOCABULARY_WORDS
    );
    assert_eq!(normalized.dictation.vocabulary[0].word, "word-0");
}

#[test]
fn dictation_vocabulary_reads_legacy_word_lists() {
    // Settings written while the vocabulary was a plain word list keep
    // working: bare strings become entries without a meaning.
    let legacy = serde_json::json!({
        "schemaVersion": 3,
        "dictation": { "vocabulary": ["Kivo", {"word": "SOC 2", "meaning": "compliance"}] },
    });
    let settings: AppSettings = serde_json::from_value(legacy).unwrap();
    let settings = settings.validate_and_normalize().unwrap();
    assert_eq!(settings.dictation.vocabulary.len(), 2);
    assert_eq!(settings.dictation.vocabulary[0].word, "Kivo");
    assert_eq!(settings.dictation.vocabulary[0].meaning, "");
    assert_eq!(settings.dictation.vocabulary[1].word, "SOC 2");
    assert_eq!(settings.dictation.vocabulary[1].meaning, "compliance");
}
