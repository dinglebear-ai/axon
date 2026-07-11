//! Resource-tier read routes added for WS-G (#298): ledger source detail,
//! the read-only route resolver preview, and the provider health/capability
//! projection. Split out of `routing.rs` to keep that file under the
//! monolith line cap — these are still mounted under `read_routes()`'s
//! `axon:read` scope guard, not a separate auth policy.

use crate::server::handlers;
use crate::server::state::AppState;
use axon_core::config::Config;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

type ServeState = (AppState, Arc<Config>);

pub(super) fn routes() -> Router<ServeState> {
    Router::new()
        .route(
            "/v1/sources/{source_id}",
            get(handlers::sources_resource::get_source),
        )
        .route(
            "/v1/resolve",
            post(handlers::sources_resource::resolve_source),
        )
        .route("/v1/providers", get(handlers::providers::list_providers))
        .route(
            "/v1/providers/{provider}",
            get(handlers::providers::get_provider),
        )
}
