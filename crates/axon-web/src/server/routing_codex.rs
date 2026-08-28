use crate::server::handlers;
use crate::server::state::AppState;
use axon_core::config::Config;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

type ServeState = (AppState, Arc<Config>);

pub(super) fn read_routes() -> Router<ServeState> {
    Router::new()
        .route("/v1/codex", get(handlers::codex_control::snapshot))
        .route("/v1/codex/events", get(handlers::codex_control::events))
        .route(
            "/v1/codex/{resource}",
            get(handlers::codex_control::resource),
        )
}

pub(super) fn admin_routes() -> Router<ServeState> {
    Router::new()
        .route(
            "/v1/codex/operations",
            post(handlers::codex_control::create_operation),
        )
        .route(
            "/v1/codex/operations/{id}/approve",
            post(handlers::codex_control::approve_operation),
        )
        .route(
            "/v1/codex/operations/{id}/execute",
            post(handlers::codex_control::execute_operation),
        )
}
