//! CLI registry data: compose/setup/mcp/migrate/config/sync/update/palette command families.
//! Split out of `cli_registry.rs` to stay under the repo's 500-line file cap; see that file for the shared `CliRegistryCommand` type and module docs.
//! Further split into per-family functions to stay under the 120-line function cap.
use super::{CliRegistryCommand, c};

pub(super) fn commands() -> Vec<CliRegistryCommand> {
    let mut commands = commands_compose_setup();
    commands.extend(commands_mcp_migrate_config());
    commands
}

fn commands_compose_setup() -> Vec<CliRegistryCommand> {
    vec![
        // compose
        c(
            &["compose", "up"],
            "Pull and start the Docker service stack",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["compose", "down"],
            "Stop the Docker service stack",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["compose", "restart"],
            "Restart running services",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["compose", "rebuild"],
            "Rebuild the Axon image and start the stack",
            None,
            true,
            false,
            "admin",
        ),
        // setup
        c(
            &["setup", "plugin-hook"],
            "Hook-safe preflight/setup entrypoint for Claude Code plugin SessionStart",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "init"],
            "Initialize local Axon config, env, and compose assets",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "check"],
            "Check local prerequisites without mutating files or services",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["setup", "targets"],
            "List SSH host aliases discovered from ~/.ssh/config",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["setup", "install"],
            "Copy the axon binary into ~/.local/bin for terminal use",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "config", "rewrite"],
            "Preview or apply clean-break config key rewrites",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "session-watch-service", "install"],
            "Write service files, run initial ingest, and enable the user service",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "session-watch-service", "check"],
            "Verify generated files and systemd state without mutating service files",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["setup", "session-watch-service", "remove"],
            "Disable the user service and remove generated service files",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["setup", "session-watch-service", "status"],
            "Print current user systemd status for the service",
            None,
            false,
            false,
            "admin",
        ),
    ]
}

fn commands_mcp_migrate_config() -> Vec<CliRegistryCommand> {
    vec![
        // mcp
        c(
            &["mcp"],
            "Start MCP stdio or unified HTTP runtime",
            None,
            false,
            false,
            "admin",
        ),
        // migrate
        c(
            &["migrate"],
            "Migrate an unnamed-vector collection to named-mode (enables hybrid RRF search)",
            None,
            true,
            false,
            "admin",
        ),
        // config
        c(
            &["config", "list"],
            "List every entry from .env and config.toml (secrets redacted)",
            Some("ConfigProjectionRequest"),
            false,
            false,
            "admin",
        ),
        c(
            &["config", "get"],
            "Print a single config value (auto-detects file by key shape)",
            Some("ConfigProjectionRequest"),
            false,
            false,
            "admin",
        ),
        c(
            &["config", "set"],
            "Write a config value (auto-detects file by key shape)",
            Some("ConfigProjectionRequest"),
            true,
            false,
            "admin",
        ),
        c(
            &["config", "unset"],
            "Remove a config value from .env or config.toml",
            Some("ConfigProjectionRequest"),
            true,
            false,
            "admin",
        ),
        c(
            &["config", "path"],
            "Print resolved paths to .env and config.toml",
            None,
            false,
            false,
            "admin",
        ),
        // sync
        c(
            &["sync", "pending"],
            "Show local artifacts waiting to be reconciled with the server",
            None,
            false,
            false,
            "read",
        ),
        // update / palette
        c(
            &["update"],
            "Download and install the latest GitHub Release binary, then sync the local container",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["palette"],
            "Resolve, launch, and optionally install the axon-palette desktop binary",
            None,
            false,
            false,
            "read",
        ),
    ]
}
