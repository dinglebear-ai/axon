#[path = "web/auth.rs"]
pub(crate) mod auth;
#[path = "web/health.rs"]
pub(crate) mod health;
#[path = "web/panel_first_run.rs"]
pub(crate) mod panel_first_run;
#[path = "web/panel_stack.rs"]
pub(crate) mod panel_stack;
#[path = "web/security.rs"]
pub(crate) mod security;
#[path = "web/server.rs"]
pub(crate) mod server;
#[path = "web/static_assets.rs"]
pub(crate) mod static_assets;

pub(crate) use server::{PanelRuntimeState, router};

pub fn openapi_document() -> utoipa::openapi::OpenApi {
    server::openapi_document()
}
