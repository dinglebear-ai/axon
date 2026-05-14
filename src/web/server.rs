use crate::services::context::ServiceContext;
use axum::Router;
use std::sync::Arc;

// Module declarations
#[path = "server/handlers.rs"]
mod handlers;
#[path = "server/routing.rs"]
mod routing;
#[path = "server/state.rs"]
mod state;
#[path = "server/types.rs"]
mod types;
#[path = "server/utils.rs"]
mod utils;

// Re-export public types and state
pub(super) use state::AppState;
pub(crate) use state::PanelRuntimeState;
pub(super) use utils::authorized;

pub(crate) fn router(
    cfg: Arc<crate::core::config::Config>,
    panel: Arc<PanelRuntimeState>,
    service_context: Arc<tokio::sync::OnceCell<Arc<ServiceContext>>>,
    auth_policy: crate::mcp::auth::AuthPolicy,
) -> Router {
    routing::router(cfg, panel, service_context, auth_policy)
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;

#[cfg(test)]
use handlers::ask::classify_ask_error;
#[cfg(test)]
use routing::ask_router;
