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
    pub(crate) codex_control:
        Result<Option<Arc<axon_services::codex_control::CodexControlService>>, String>,
}

pub(crate) fn build_codex_control(
    cfg: &axon_core::config::Config,
) -> Result<Option<Arc<axon_services::codex_control::CodexControlService>>, String> {
    if !cfg.codex_control_enabled {
        return Ok(None);
    }
    let home = cfg.codex_control_home.clone().ok_or_else(|| {
        "AXON_CODEX_CONTROL_HOME is required when Codex control is enabled".to_string()
    })?;
    let control = axon_codex::control::ControlConfig {
        enabled: true,
        codex_binary: std::path::PathBuf::from(&cfg.codex_cmd),
        control_home: home,
        request_timeout: std::time::Duration::from_secs(cfg.llm_completion_timeout_secs.max(1)),
        read_concurrency: cfg.codex_completion_concurrency.max(1),
        max_restart_backoff: std::time::Duration::from_secs(60),
    };
    let policy = axon_codex::api::WritePolicy {
        account: cfg.codex_control_account_writes,
        config: cfg.codex_control_config_writes,
        mcp: cfg.codex_control_mcp_writes,
        plugins: cfg.codex_control_plugin_writes,
        skills: cfg.codex_control_skill_writes,
        imports: cfg.codex_control_skill_writes,
    };
    let database = cfg.sqlite_path.with_file_name("codex-control.db");
    axon_services::codex_control::CodexControlService::new(control, policy, &database)
        .map(Arc::new)
        .map(Some)
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
