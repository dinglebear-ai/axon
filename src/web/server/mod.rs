use crate::services::context::ServiceContext;
use axum::Router;
use std::sync::Arc;

// Module declarations
mod handlers;
mod routing;
mod state;
mod types;
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

pub(crate) use utils::warn_if_ask_token_set_but_empty;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
