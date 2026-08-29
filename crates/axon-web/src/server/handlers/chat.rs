use super::super::error::HttpError;
use axon_services::client_contract::{RestChatRequest, RestChatResponse};
use axon_services::context::ServiceContext;
use axon_services::service_traits::{AskService, AskServiceImpl};
use axum::{Extension, Json, http::StatusCode, response::IntoResponse};
use lab_auth::AuthContext;
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

#[utoipa::path(
    post,
    path = "/v1/chat",
    request_body = RestChatRequest,
    responses(
        (status = 200, description = "Direct LLM chat answer", body = RestChatResponse),
        (status = 400, description = "Invalid chat request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Chat request exceeds limits", body = crate::server::error::ErrorBody),
        (status = 502, description = "Configured LLM backend unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_chat(
    Extension(context): Extension<Arc<ServiceContext>>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<RestChatRequest>,
) -> impl IntoResponse {
    if let Err(err) = validate_chat_message(&req.message) {
        return err.into_response();
    }

    if let Some(options) = req.agent.clone() {
        let Some(binding) = req.loadout.as_ref() else {
            return HttpError::bad_request("agent mode requires a revision-bound loadout")
                .into_response();
        };
        let resolved = match axon_services::loadout_context::resolve(context.cfg(), binding).await {
            Ok(value) => value,
            Err(error) => {
                return HttpError::new(
                    StatusCode::BAD_GATEWAY,
                    "loadout_resolution_failed",
                    error.to_string(),
                )
                .into_response();
            }
        };
        let cfg = context.cfg().clone();
        let completion = axon_services::agent_runtime::configured_completion(cfg.clone());
        return match axon_services::agent_runtime::run(
            &cfg,
            &binding.loadout_id,
            resolved.metadata.effective_revision,
            &format!("{}\n\n{}", resolved.prompt_context, req.message),
            options,
            axon_services::agent_runtime::AgentTurnOwner {
                principal: auth
                    .as_ref()
                    .map(|v| v.sub.clone())
                    .unwrap_or_else(|| "loopback-local".into()),
                profile_id: binding.integration_id.clone(),
            },
            completion,
        )
        .await
        {
            Ok(agent) => Json(RestChatResponse {
                message: req.message,
                answer: agent.answer.clone().unwrap_or_default(),
                model: axon_core::llm::configured_chat_model_from_config(&cfg),
                loadout: Some(resolved.metadata),
                agent: Some(agent),
            })
            .into_response(),
            Err(error) => HttpError::new(
                StatusCode::BAD_GATEWAY,
                "agent_runtime_failed",
                error.to_string(),
            )
            .into_response(),
        };
    }

    match AskServiceImpl::new(context)
        .chat(axon_services::service_traits::ask_service::ChatRequest {
            session_id: req.session_id.clone(),
            message: req.message.clone(),
            loadout: req.loadout.clone(),
            agent: req.agent.clone(),
        })
        .await
    {
        Ok(completion) => Json(RestChatResponse {
            message: req.message,
            answer: completion.reply,
            model: completion.model,
            loadout: completion.loadout,
            agent: completion.agent,
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
