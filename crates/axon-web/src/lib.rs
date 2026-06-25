#![recursion_limit = "512"]

#[path = "auth.rs"]
pub mod auth;
#[path = "health.rs"]
pub mod health;
#[path = "metrics.rs"]
pub mod metrics;
#[path = "panel_first_run.rs"]
pub mod panel_first_run;
#[path = "panel_stack.rs"]
pub mod panel_stack;
#[path = "security.rs"]
pub mod security;
#[path = "server.rs"]
pub mod server;
#[path = "static_assets.rs"]
pub mod static_assets;

pub use server::{PanelRuntimeState, router};

pub fn openapi_document() -> utoipa::openapi::OpenApi {
    server::openapi_document()
}
