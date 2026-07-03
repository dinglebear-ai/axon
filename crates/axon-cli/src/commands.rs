pub mod ask;
pub mod brand;
pub mod common;
mod common_jobs;
pub mod common_urls;
pub mod completions;
pub mod config;
pub mod debug;
pub mod dedupe;
pub mod diff;
pub mod doctor;
pub mod domains;
pub mod endpoints;
pub mod evaluate;
pub mod extract;
pub mod fresh;
pub mod ingest_common;
pub mod job_contracts;
mod job_progress;
pub mod map;
pub mod mcp;
pub mod memory;
pub mod migrate;
pub mod monitor;
pub mod palette;
pub mod probe;
pub mod purge;
pub mod query;
pub mod refresh;
pub mod research;
pub mod retrieve;
pub mod screenshot;
pub mod search;
pub mod serve;
pub mod sessions;
pub mod setup;
pub mod source;
pub mod sources;
pub mod stats;
pub mod status;
pub mod suggest;
pub mod summarize;
pub mod sync;
pub mod train;
pub mod unified_server;
pub mod update;
pub mod watch;

#[cfg(test)]
mod services_migration_tests;

pub use ask::run_ask;
pub use brand::run_brand;
pub use common::start_url_from_cfg;
pub use completions::run_completions;
pub use config::run_config;
pub use debug::run_debug;
pub use dedupe::run_dedupe;
pub use diff::run_diff;
pub use doctor::run_doctor;
pub use domains::run_domains;
pub use endpoints::run_endpoints;
pub use evaluate::run_evaluate;
pub use extract::run_extract;
pub use fresh::run_fresh;
pub use map::run_map;
pub use mcp::run_mcp;
pub use memory::run_memory;
pub use migrate::run_migrate;
pub use monitor::run_monitor;
pub use palette::run_palette;
pub use purge::run_purge;
pub use query::run_query;
pub use refresh::run_refresh;
pub use research::run_research;
pub use retrieve::run_retrieve;
pub use screenshot::run_screenshot;
pub use search::run_search;
pub use serve::run_serve;
pub use sessions::run_sessions;
pub use setup::{apply_plugin_options, run_setup};
pub use source::run_source;
pub use sources::run_sources;
pub use stats::run_stats;
pub use status::run_status;
pub use suggest::run_suggest;
pub use summarize::run_summarize;
pub use sync::run_sync;
pub use train::run_train;
pub use unified_server::run_unified_server;
pub use update::run_update;
pub use watch::run_watch;

use axon_core::config::Config;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;

pub(crate) type CommandFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>>;

/// Resolve free-text input from `--query` flag or positional args, trimming whitespace.
/// Returns `None` if both sources are empty or whitespace-only.
///
/// Shared by ask, query, evaluate, suggest, search, and research commands.
pub(crate) fn resolve_input_text(cfg: &Config) -> Option<String> {
    cfg.query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            let joined = cfg.positional.join(" ");
            let trimmed = joined.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        })
}

#[cfg(test)]
#[path = "commands_command_signature_tests.rs"]
mod command_signature_tests;
