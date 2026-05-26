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
