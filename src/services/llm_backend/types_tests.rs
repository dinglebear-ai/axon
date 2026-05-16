use super::*;

#[test]
fn backend_config_ignores_legacy_openai_model_names() {
    let cfg = Config {
        openai_model: "gpt-4o-mini".to_string(),
        ..Config::default()
    };
    let backend = LlmBackendConfig::from_config(&cfg);
    assert_eq!(backend.gemini_model, None);
}

#[test]
fn backend_config_accepts_explicit_gemini_model() {
    let cfg = Config {
        openai_model: "gpt-4o-mini".to_string(),
        headless_gemini_model: "gemini-3.1-pro-preview".to_string(),
        ..Config::default()
    };
    let backend = LlmBackendConfig::from_config(&cfg);
    assert_eq!(
        backend.gemini_model.as_deref(),
        Some("gemini-3.1-pro-preview")
    );
}
