//! Hand-maintained CLI command registry used by the `cli` schema family.
//!
//! `axon-cli`/`axon-core` are outside this territory's write access, so this
//! registry is maintained here in `xtask` rather than as a
//! `crates/axon-cli/src/schema_registry.rs` addition. It mirrors the real
//! clap command tree in `crates/axon-core/src/config/cli.rs` (the
//! `CliCommand` enum plus its subcommand enums), grouped into top-level
//! commands and subcommand families.
//!
//! Coverage rule: every entry here must correspond to a real, live clap
//! variant. Commands the removed-surface contract forbids from CLI docs
//! (`embed`, `ingest`, `scrape`, `crawl`, `code-search`, `code-search-watch`,
//! `purge`, `dedupe`, `axon refresh`, `fresh` — see
//! `xtask/src/schemas/registry.rs::REMOVED_SURFACE_RULES`) are intentionally
//! excluded even though some of them (`refresh`, `fresh`) are still
//! dispatchable today; the contract says they must not appear in generated
//! docs. Target-only command groups from the Phase #298 clean-break contract
//! that do not exist as real clap commands yet (`graph`, `providers`,
//! `collections`, `artifacts`, `uploads`, `capabilities`, `chat`) are
//! likewise NOT fabricated here — see `docs/pipeline-unification/schemas/
//! cli-schema.md` for that target shape, and
//! `xtask/src/schemas/cli_registry_tests.rs` for the cross-check against the
//! live clap source.
//!
//! The actual command data is split across `cli_registry/part{1,2,3}.rs` to
//! stay under the repo's 500-line file cap.
use serde_json::{Value, json};

#[path = "cli_registry/part1.rs"]
mod part1;
#[path = "cli_registry/part2.rs"]
mod part2;
#[path = "cli_registry/part3.rs"]
mod part3;

/// One CLI command or grouped-subcommand record.
pub(super) struct CliRegistryCommand {
    /// Command path, e.g. `&["watch", "create"]` or `&["ask"]`.
    pub path: &'static [&'static str],
    pub summary: &'static str,
    pub maps_to_dto: Option<&'static str>,
    pub mutates: bool,
    pub async_job: bool,
    pub requires_auth_scope: &'static str,
}

/// Compact constructor used by the `cli_registry/part*.rs` data files.
pub(super) fn c(
    path: &'static [&'static str],
    summary: &'static str,
    maps_to_dto: Option<&'static str>,
    mutates: bool,
    async_job: bool,
    requires_auth_scope: &'static str,
) -> CliRegistryCommand {
    CliRegistryCommand {
        path,
        summary,
        maps_to_dto,
        mutates,
        async_job,
        requires_auth_scope,
    }
}

/// Full command registry, grouped by top-level command family. Ordered to
/// match declaration order in `CliCommand` (see module docs) for easy diffing
/// against the source enum.
pub(super) fn command_registry() -> Vec<CliRegistryCommand> {
    let mut commands = part1::commands();
    commands.extend(part2::commands());
    commands.extend(part3::commands());
    commands
}

/// Top-level command groups that exist as real, dispatchable `CliCommand`
/// variants but are excluded from generated CLI docs per
/// `REMOVED_SURFACE_RULES` (see module docs for why `refresh`/`fresh`/
/// `purge`/`dedupe` are listed even though they are still live).
#[allow(dead_code)]
pub(super) fn excluded_top_level_groups() -> &'static [&'static str] {
    &[
        "embed",
        "ingest",
        "scrape",
        "crawl",
        "code-search",
        "code-search-watch",
        "purge",
        "dedupe",
        "refresh",
        "fresh",
    ]
}

pub(super) fn command_records() -> Vec<Value> {
    command_registry()
        .iter()
        .map(|command| {
            let name = command.path.join(" ");
            json!({
                "name": name,
                "path": command.path,
                "group": command.path[0],
                "summary": command.summary,
                "maps_to_dto": command.maps_to_dto,
                "mutates": command.mutates,
                "async": command.async_job,
                "requires_auth_scope": command.requires_auth_scope,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "cli_registry_tests.rs"]
mod tests;
