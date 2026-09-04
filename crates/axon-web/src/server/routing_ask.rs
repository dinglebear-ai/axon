use super::handlers;
use crate::server::types::ASK_BODY_LIMIT;
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axum::{
    Extension, Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use std::sync::Arc;

pub(crate) fn ask_router<S>(cfg: Arc<Config>, service_context: Arc<ServiceContext>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::<S>::new()
        .route("/v1/ask", post(handlers::v1_ask))
        .route("/v1/ask/stream", post(handlers::v1_ask_stream))
        .route("/v1/chat", post(handlers::v1_chat))
        .route("/v1/chat/stream", post(handlers::v1_chat_stream))
        .route("/v1/agent/turns/{id}", get(handlers::v1_agent_status))
        .route(
            "/v1/agent/turns/{id}/events",
            get(handlers::v1_agent_events),
        )
        .route(
            "/v1/agent/turns/{id}/cancel",
            post(handlers::v1_agent_cancel),
        )
        .route(
            "/v1/agent/turns/{id}/resume",
            post(handlers::v1_agent_resume),
        )
        .layer(DefaultBodyLimit::max(ASK_BODY_LIMIT))
        .layer(Extension(service_context))
        .layer(Extension(cfg))
}
