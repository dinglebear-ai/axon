use super::*;
use std::path::PathBuf;

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
fn backend_kind_parses_codex_app_server_aliases() {
    for alias in ["codex-app-server", "codex_app_server", "codex"] {
        assert_eq!(
            LlmBackendKind::parse(alias).unwrap(),
            LlmBackendKind::CodexAppServer
        );
    }
}

#[test]
fn backend_config_accepts_codex_fields() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "/usr/local/bin/codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        codex_home: Some(PathBuf::from("/home/example/.codex")),
        codex_completion_concurrency: 1,
        ..Config::default()
    };

    let backend = LlmBackendConfig::from_config(&cfg);

    assert_eq!(backend.kind, LlmBackendKind::CodexAppServer);
    assert_eq!(backend.codex_cmd, "/usr/local/bin/codex");
    assert_eq!(backend.codex_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        backend.codex_home,
        Some(PathBuf::from("/home/example/.codex"))
    );
    assert_eq!(backend.completion_concurrency, 1);
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

#[test]
fn configured_model_uses_codex_backend_model() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        headless_gemini_model: "gemini-should-not-win".to_string(),
        openai_model: "openai-should-not-win".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    let synthesis = CompletionRequest::new("hello").backend_from_config(&cfg);

    assert_eq!(synthesis.model.as_deref(), Some("gpt-5.5"));
}

#[test]
fn completion_timeout_is_at_least_one_second_on_backend_config() {
    let zero = LlmBackendConfig {
        completion_timeout_secs: 0,
        ..LlmBackendConfig::default()
    };
    assert_eq!(zero.completion_timeout(), std::time::Duration::from_secs(1));
}

#[test]
fn chat_model_uses_chat_override_for_openai_compat_backend() {
    let cfg = Config {
        llm_backend: LlmBackendKind::OpenAiCompat,
        openai_model: "synthesis-model".to_string(),
        openai_chat_model: "chat-model".to_string(),
        ..Config::default()
    };

    let req = CompletionRequest::new("hello").backend_from_config_for(&cfg, LlmModelPurpose::Chat);

    assert_eq!(req.model.as_deref(), Some("chat-model"));
}

#[test]
fn chat_model_falls_back_to_synthesis_model_when_unset() {
    let cfg = Config {
        llm_backend: LlmBackendKind::GeminiHeadless,
        headless_gemini_model: "gemini-synthesis".to_string(),
        ..Config::default()
    };

    let req = CompletionRequest::new("hello").backend_from_config_for(&cfg, LlmModelPurpose::Chat);

    assert_eq!(req.model.as_deref(), Some("gemini-synthesis"));
}
