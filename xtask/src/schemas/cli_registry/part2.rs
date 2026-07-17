//! CLI registry data: sources/domains/stats/status/jobs/memory/sessions/source/screenshot/completions/serve/reset/prune/preflight/smoke command families.
//! Split out of `cli_registry.rs` to stay under the repo's 500-line file cap; see that file for the shared `CliRegistryCommand` type and module docs.
//! Further split into per-family functions to stay under the 120-line function cap.
use super::{CliRegistryCommand, c};

pub(super) fn commands() -> Vec<CliRegistryCommand> {
    let mut commands = commands_sources_stats_jobs();
    commands.extend(commands_memory());
    commands.extend(commands_sessions());
    commands.extend(commands_source_serve_prune());
    commands
}

fn commands_sources_stats_jobs() -> Vec<CliRegistryCommand> {
    vec![
        // sources / domains / stats / status
        c(
            &["sources"],
            "List all indexed source URLs with chunk counts",
            Some("SourcesRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["domains"],
            "List indexed domains with document statistics",
            Some("DomainsRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["stats"],
            "Show Qdrant collection and SQLite job statistics",
            Some("StatsRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["status"],
            "Show unified jobs, watches, cleanup, totals, and service status",
            None,
            false,
            false,
            "read",
        ),
        // jobs
        c(
            &["jobs", "list"],
            "List unified durable jobs",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["jobs", "get"],
            "Show one unified durable job",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["jobs", "events"],
            "Show one job's event page",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["jobs", "stream"],
            "Fetch an event page for stream consumers",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["jobs", "cancel"],
            "Request cancellation for a unified durable job",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["jobs", "retry"],
            "Retry a unified durable job",
            None,
            true,
            true,
            "write",
        ),
        c(
            &["jobs", "recover"],
            "Recover stale unified durable jobs",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["jobs", "cleanup"],
            "Remove old terminal unified durable jobs",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["jobs", "clear"],
            "Clear all unified durable job rows",
            None,
            true,
            false,
            "admin",
        ),
        c(
            &["jobs", "worker"],
            "Run a standalone worker process for the unified durable queue",
            None,
            true,
            false,
            // admin: the worker performs the same stale-job reclaim as
            // `jobs recover` (admin) automatically on a timer, so it is at
            // least as privileged (axon_rust-x4gxr.13).
            "admin",
        ),
    ]
}

fn commands_memory() -> Vec<CliRegistryCommand> {
    vec![
        // memory
        c(
            &["memory", "remember"],
            "Store a memory in the dedicated memory collection",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["memory", "list"],
            "List memory metadata without semantic search",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["memory", "search"],
            "Search active memories",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["memory", "show"],
            "Show one memory by id",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["memory", "link"],
            "Link two memories in the SQLite graph",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["memory", "supersede"],
            "Mark an old memory as superseded by a replacement memory",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["memory", "context"],
            "Build an inline, defanged context block from memories",
            None,
            false,
            false,
            "read",
        ),
    ]
}

fn commands_sessions() -> Vec<CliRegistryCommand> {
    vec![c(
        &["sessions"],
        "Index AI session exports (Claude, Codex, Gemini) into Qdrant",
        None,
        true,
        true,
        "write",
    )]
}

fn commands_source_serve_prune() -> Vec<CliRegistryCommand> {
    vec![
        // source (unified local-path indexing)
        c(
            &["source"],
            "Index a source through the unified pipeline",
            None,
            true,
            true,
            "write",
        ),
        // screenshot
        c(
            &["screenshot"],
            "Capture a full-page screenshot of one or more URLs",
            None,
            true,
            false,
            "write",
        ),
        // completions
        c(
            &["completions"],
            "Generate shell completions (bash, zsh, fish)",
            None,
            false,
            false,
            "read",
        ),
        // serve
        c(
            &["serve"],
            "Start service runtimes",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["serve", "mcp"],
            "Start unified web + MCP HTTP runtime",
            None,
            false,
            false,
            "admin",
        ),
        // reset
        c(
            &["reset", "plan"],
            "Create a reviewable clean-slate reset plan without deleting data",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["reset", "exec"],
            "Execute a reviewed clean-slate reset plan",
            None,
            true,
            false,
            "admin",
        ),
        // prune
        c(
            &["prune", "plan"],
            "Resolve a prune target into a reviewable dry-run plan",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["prune", "exec"],
            "Execute a prune target's plan (destructive; requires --confirm)",
            None,
            true,
            false,
            "admin",
        ),
        // preflight / smoke
        c(
            &["preflight"],
            "Check host prerequisites and service readiness",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["smoke"],
            "Run source/ask smoke checks against the running stack",
            None,
            true,
            false,
            "admin",
        ),
    ]
}
