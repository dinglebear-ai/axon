use super::super::auth::{PanelPassword, init_panel_password};
use axon_services::context::ServiceContext;
use std::sync::Arc;

#[derive(Clone)]
pub struct PanelRuntimeState {
    pub(super) password: PanelPassword,
    pub(super) setup_required: bool,
    pub(super) config_path: String,
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) panel: Arc<PanelRuntimeState>,
    pub(crate) service_context: Arc<ServiceContext>,
}

impl PanelRuntimeState {
    pub fn initialize(host: &str, port: u16) -> std::io::Result<Self> {
        super::utils::warn_if_ask_token_set_but_empty();
        let config_init = axon_services::setup::config_store::ensure_user_config()?;
        let password_init = init_panel_password()?;
        if password_init.generated {
            eprintln!(
                "Axon web panel password: {}\nOpen: http://{}:{}",
                password_init.password.as_str(),
                host,
                port
            );
        }
        Ok(Self {
            password: password_init.password,
            setup_required: config_init.created,
            config_path: config_init.path.display().to_string(),
        })
    }

    pub fn setup_required(&self) -> bool {
        self.setup_required
    }
}
