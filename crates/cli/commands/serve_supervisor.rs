#[path = "serve_supervisor_model.rs"]
mod serve_supervisor_model;
#[path = "serve_supervisor_preflight.rs"]
mod serve_supervisor_preflight;
#[path = "serve_supervisor_runtime.rs"]
mod serve_supervisor_runtime;

use crate::crates::core::config::Config;
use std::error::Error;

#[cfg(test)]
#[allow(unused_imports)]
use serve_supervisor_model::{
    ANSI_BLUE, ANSI_CYAN, ANSI_GREEN, ANSI_MAGENTA, ANSI_RED, ANSI_YELLOW, ChildSpec,
    ComposeServiceStatus, DOCKER_SERVICES_FILE, MAX_UNSTABLE_RESTARTS, PortBinding, PortOwner,
    RESTART_BACKOFF_INITIAL_SECS, RESTART_BACKOFF_MAX_SECS, RESTART_STABLE_WINDOW_SECS,
    SERVE_CHILD_ROLE_BRIDGE, SERVE_CHILD_ROLE_ENV, SHUTDOWN_GRACE_SECS,
};
#[cfg(test)]
#[allow(unused_imports)]
use serve_supervisor_preflight::{
    extract_pids_from_ss_line, graph_worker_enabled, inspect_compose_services, inspect_port_owners,
    inspect_process_command, inspect_processes_matching, parse_matching_processes,
    port_owner_matches_binding, preflight_dependencies, reconcile_nextjs_dev_lock,
    reconcile_required_ports, require_command, required_bind_ports, required_container_services,
    supervised_child_specs, terminate_port_owner,
};
#[cfg(test)]
#[allow(unused_imports)]
use serve_supervisor_runtime::{
    log_child_event, log_stream_line, log_supervisor, next_restart_delay,
    reached_unstable_restart_limit, spawn_child,
};

pub(super) fn is_internal_bridge_runtime() -> bool {
    serve_supervisor_runtime::is_internal_bridge_runtime()
}

pub(super) async fn run_supervisor(cfg: &Config) -> Result<(), Box<dyn Error>> {
    serve_supervisor_runtime::run_supervisor(cfg).await
}

#[cfg(test)]
#[path = "serve_supervisor_tests.rs"]
mod tests;
