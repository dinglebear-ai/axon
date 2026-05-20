use crate::services::context::ServiceContext;
use axum::Router;
use std::sync::Arc;

// Module declarations
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
pub(super) use state::AppState;
pub(crate) use state::PanelRuntimeState;
pub(super) use utils::authorized;

pub(crate) fn router(
    cfg: Arc<crate::core::config::Config>,
    panel: Arc<PanelRuntimeState>,
    service_context: Arc<ServiceContext>,
    auth_policy: crate::mcp::auth::AuthPolicy,
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
