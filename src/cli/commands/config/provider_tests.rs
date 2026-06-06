use super::*;

#[test]
fn provider_names_extracts_unique_sorted_names() {
    let flat = BTreeMap::from([
        (
            "providers.codex.backend".to_string(),
            "codex-app-server".to_string(),
        ),
        ("providers.codex.model".to_string(), "gpt-5.5".to_string()),
        (
            "providers.gem.backend".to_string(),
            "gemini-headless".to_string(),
        ),
        ("search.collection".to_string(), "axon".to_string()),
    ]);
    assert_eq!(
        provider_names(&flat),
        vec!["codex".to_string(), "gem".to_string()]
    );
}

#[test]
fn provider_names_empty_when_no_providers() {
    let flat = BTreeMap::from([("ask.full-docs".to_string(), "6".to_string())]);
    assert!(provider_names(&flat).is_empty());
}

#[test]
fn validate_field_accepts_known_fields() {
    for f in ["backend", "model", "base-url", "api-key", "cmd", "home"] {
        assert!(validate_field(f).is_ok(), "{f} should be valid");
    }
}

#[test]
fn validate_field_rejects_unknown() {
    let err = validate_field("modle").unwrap_err().to_string();
    assert!(err.contains("unknown provider field"));
    assert!(err.contains("model"));
}

#[test]
fn backend_label_round_trips_all_kinds() {
    assert_eq!(
        backend_label(LlmBackendKind::GeminiHeadless),
        "gemini-headless"
    );
    assert_eq!(backend_label(LlmBackendKind::OpenAiCompat), "openai-compat");
    assert_eq!(
        backend_label(LlmBackendKind::CodexAppServer),
        "codex-app-server"
    );
    // Labels must round-trip back through the parser.
    for label in ["gemini-headless", "openai-compat", "codex-app-server"] {
        assert!(LlmBackendKind::parse(label).is_ok());
    }
}
