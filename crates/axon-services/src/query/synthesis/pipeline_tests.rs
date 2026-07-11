use super::*;
use axon_llm::LlmBackendKind;

#[test]
fn validate_ask_llm_config_accepts_default_gemini_config() {
    let cfg = Config::test_default();

    let result = validate_ask_llm_config(&cfg);

    assert!(result.is_ok(), "Gemini config should pass validation");
}

#[test]
fn validate_ask_llm_config_accepts_openai_compat_config() {
    let mut cfg = Config::test_default();
    cfg.llm_backend = LlmBackendKind::OpenAiCompat;
    cfg.openai_base_url = "http://llama-cpp:8080/v1".to_string();
    cfg.openai_model = "gemma".to_string();

    let result = validate_ask_llm_config(&cfg);

    assert!(
        result.is_ok(),
        "OpenAI-compatible config should pass validation"
    );
}

#[test]
fn validate_ask_llm_config_rejects_openai_compat_without_base_url() {
    let mut cfg = Config::test_default();
    cfg.llm_backend = LlmBackendKind::OpenAiCompat;
    cfg.openai_model = "gemma".to_string();

    let err = validate_ask_llm_config(&cfg).expect_err("base URL should be required");

    assert!(err.to_string().contains("AXON_OPENAI_BASE_URL"));
}

#[test]
fn validate_ask_llm_config_accepts_codex_app_server_config() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    validate_ask_llm_config(&cfg).expect("codex config should validate");
}

#[test]
fn validate_ask_llm_config_rejects_empty_codex_cmd() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "   ".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    let err = validate_ask_llm_config(&cfg).unwrap_err();
    assert!(err.to_string().contains("AXON_CODEX_CMD"));
}

#[test]
fn ask_context_with_follow_up_appends_history_to_existing_context() {
    let cfg = Config {
        ask_follow_up_context: Some("Previous Q&A".to_string()),
        ..Config::default()
    };

    let combined = ask_context_with_follow_up(&cfg, "Sources:\n## Top Chunk [S1]: x");

    assert!(combined.starts_with("Sources:\n## Top Chunk [S1]: x"));
    assert!(combined.ends_with("Previous Q&A"));
}

#[test]
fn ask_context_with_follow_up_seeds_sources_header_when_context_empty() {
    let cfg = Config {
        ask_follow_up_context: Some("Previous Q&A".to_string()),
        ..Config::default()
    };

    let combined = ask_context_with_follow_up(&cfg, "");

    assert_eq!(combined, "Sources:\nPrevious Q&A");
}

#[test]
fn ask_context_with_follow_up_passes_through_when_no_history() {
    let cfg = Config::default();
    let combined = ask_context_with_follow_up(&cfg, "Sources:\n## Top Chunk [S1]: x");
    assert_eq!(combined, "Sources:\n## Top Chunk [S1]: x");
}

#[test]
fn normalized_stream_correction_labels_stored_normalized_answer() {
    let rendered = normalized_stream_correction_text(
        "Answer with normalized citations [S1].\n\n## Sources\n- [S1] https://docs.example.com",
    );

    assert!(rendered.contains("Normalized answer (stored for JSON and follow-up sessions):"));
    assert!(rendered.contains("Answer with normalized citations [S1]."));
    assert!(rendered.starts_with("\n\n---\n\n"));
}
