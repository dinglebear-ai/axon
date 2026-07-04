use axon_services::context::ServiceContext;
use axum::Router;
use std::sync::Arc;

// Module declarations
#[path = "server/api_error.rs"]
mod api_error;
#[path = "server/error.rs"]
mod error;
#[path = "server/handlers.rs"]
mod handlers;
#[path = "server/openapi.rs"]
mod openapi;
#[path = "server/openapi_jobs.rs"]
mod openapi_jobs;
#[path = "server/routing.rs"]
mod routing;
#[path = "server/state.rs"]
mod state;
#[path = "server/types.rs"]
mod types;
#[path = "server/utils.rs"]
mod utils;

// Re-export public types and state
pub(super) use error::HttpError;
pub(crate) use openapi::openapi_document;
pub(super) use state::AppState;
pub use state::PanelRuntimeState;
pub(super) use utils::authorized;

pub fn router(
    cfg: Arc<axon_core::config::Config>,
    panel: Arc<PanelRuntimeState>,
    service_context: Arc<ServiceContext>,
    auth_policy: axon_authz::http::AuthPolicy,
) -> Router {
    routing::router(cfg, panel, service_context, auth_policy)
}

#[cfg(test)]
#[path = "server_test_support_tests.rs"]
mod test_support;

#[cfg(test)]
#[path = "server_dedupe_tests.rs"]
mod dedupe_tests;

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;

#[cfg(test)]
use routing::{ScopeRequirement, ask_router, protect_routes};
