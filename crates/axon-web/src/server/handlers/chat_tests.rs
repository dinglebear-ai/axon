use axon_core::config::Config;
use axum::response::IntoResponse;

#[test]
fn v1_chat_rejects_empty_message() {
    let error = super::validate_chat_message("   ").expect_err("empty chat must fail");
    assert_eq!(
        error.into_response().status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}

#[test]
fn completion_request_has_no_rag_system_prompt() {
    let cfg = Config {
        llm_backend: axon_llm::LlmBackendKind::OpenAiCompat,
        openai_model: "synthesis-model".to_string(),
        openai_chat_model: "chat-model".to_string(),
        ..Config::default()
    };
    let request = super::completion_request(&cfg, "hello", true);

    assert_eq!(request.user_prompt, "hello");
    assert!(request.system_prompt.is_none());
    assert_eq!(request.model.as_deref(), Some("chat-model"));
    assert!(request.stream);
}
