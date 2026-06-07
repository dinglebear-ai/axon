use axum::{Json, response::IntoResponse};
use std::sync::Arc;

use crate::{core::config::Config, services::client_contract::RestChatRequest};

#[tokio::test]
async fn v1_chat_rejects_empty_message() {
    let response = super::v1_chat(
        axum::Extension(Arc::new(Config::default())),
        Json(RestChatRequest {
            message: "   ".to_string(),
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn completion_request_has_no_rag_system_prompt() {
    let cfg = Config {
        llm_backend: crate::services::llm_backend::LlmBackendKind::OpenAiCompat,
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
