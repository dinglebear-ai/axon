pub mod assets;
pub mod config_store;
pub mod diagnostics;
pub mod local;
pub mod session_watch_service;
pub mod ssh_targets;

pub use crate::core::config::SessionWatchServiceAction;
pub use local::{
    ComposeAction, LocalSetupInitOptions, LocalSetupMode, LocalSetupPhase, LocalSetupReport,
    LocalSetupStatus, run_compose_action, run_local_setup, run_local_setup_with_options,
    stack_already_healthy,
};
pub use session_watch_service::{SessionWatchServiceReport, run_session_watch_service_setup};
pub use ssh_targets::{SshTarget, list_ssh_targets};
