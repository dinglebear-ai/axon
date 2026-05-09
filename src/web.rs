#[path = "web/actions.rs"]
pub(crate) mod actions;
#[path = "web/auth.rs"]
pub(crate) mod auth;
#[path = "web/security.rs"]
pub(crate) mod security;
#[path = "web/server.rs"]
pub(crate) mod server;
#[path = "web/static_assets.rs"]
pub(crate) mod static_assets;

pub(crate) use server::{PanelRuntimeState, router};
