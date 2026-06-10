use super::super::error::HttpError;
use crate::core::config::Config;
use crate::core::llm::{self, CompletionRequest, LlmModelPurpose};
use crate::services::client_contract::{RestChatRequest, RestChatResponse};
use axum::{Extension, Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

pub(super) fn validate_chat_message(message: &str) -> Result<(), HttpError> {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if message.trim().is_empty() {
        return Err(HttpError::bad_request("message is required"));
    }
    if message.chars().count() > ASK_QUERY_MAX_CHARS {
        return Err(HttpError::payload_too_large(format!(
            "message exceeds {ASK_QUERY_MAX_CHARS} chars"
        )));
    }
    Ok(())
}

pub(super) fn completion_request(cfg: &Config, message: &str, stream: bool) -> CompletionRequest {
    CompletionRequest::new(message)
        .backend_from_config_for(cfg, LlmModelPurpose::Chat)
        .stream(stream)
}

#[utoipa::path(
    post,
    path = "/v1/chat",
    request_body = RestChatRequest,
    responses(
        (status = 200, description = "Direct LLM chat answer", body = RestChatResponse),
        (status = 400, description = "Invalid chat request", body = crate::web::server::error::ErrorBody),
        (status = 413, description = "Chat request exceeds limits", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Configured LLM backend unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_chat(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<RestChatRequest>,
) -> impl IntoResponse {
    if let Err(err) = validate_chat_message(&req.message) {
        return err.into_response();
    }

    let request = completion_request(&cfg, &req.message, false);
    let model = request.model.clone();
    match llm::complete_text(request).await {
        Ok(completion) => Json(RestChatResponse {
            message: req.message,
            answer: completion.text,
            model,
        })
        .into_response(),
        Err(err) => {
            HttpError::new(StatusCode::BAD_GATEWAY, "bad_gateway", err.to_string()).into_response()
        }
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
