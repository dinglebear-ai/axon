use super::*;

#[test]
fn backend_config_defaults_to_no_model_when_unset() {
    let cfg = Config::default();
    let backend = LlmBackendConfig::from_config(&cfg);
    assert_eq!(backend.gemini_model, None);
}

#[test]
fn backend_config_accepts_explicit_gemini_model() {
    let cfg = Config {
        headless_gemini_model: "gemini-3.1-pro-preview".to_string(),
        ..Config::default()
    };
    let backend = LlmBackendConfig::from_config(&cfg);
    assert_eq!(
        backend.gemini_model.as_deref(),
        Some("gemini-3.1-pro-preview")
    );
}

#[test]
fn as_str_returns_canonical_tokens() {
    assert_eq!(LlmBackendKind::GeminiHeadless.as_str(), "gemini-headless");
    assert_eq!(LlmBackendKind::OpenAiCompat.as_str(), "openai-compat");
    assert_eq!(LlmBackendKind::CodexAppServer.as_str(), "codex-app-server");
}

#[test]
fn as_str_round_trips_through_parse() {
    // The job snapshot serializes via as_str and workers parse it back — a typo
    // in either map would silently break backend selection for async jobs.
    for kind in [
        LlmBackendKind::GeminiHeadless,
        LlmBackendKind::OpenAiCompat,
        LlmBackendKind::CodexAppServer,
    ] {
        assert_eq!(
            LlmBackendKind::parse(kind.as_str()).unwrap(),
            kind,
            "{} must round-trip through parse",
            kind.as_str()
        );
    }
}

#[test]
fn configured_model_uses_openai_model_for_openai_compat_backend() {
    let cfg = Config {
        llm_backend: LlmBackendKind::OpenAiCompat,
        headless_gemini_model: "gemini-should-not-win".to_string(),
        openai_model: "gemma-4-e4b".to_string(),
        ..Config::default()
    };
    let req = CompletionRequest::new("hello").backend_from_config(&cfg);
    assert_eq!(req.model.as_deref(), Some("gemma-4-e4b"));
}
