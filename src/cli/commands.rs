pub mod ask;
pub mod common;
mod common_jobs;
pub mod common_urls;
pub mod completions;
pub mod config;
pub mod crawl;
pub mod debug;
pub mod dedupe;
pub mod doctor;
pub mod domains;
pub mod embed;
pub mod evaluate;
pub mod extract;
pub mod ingest;
pub mod ingest_common;
pub mod job_contracts;
pub mod map;
pub mod mcp;
pub mod migrate;
pub mod probe;
pub mod query;
pub mod research;
pub mod retrieve;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod serve;
pub mod sessions;
pub mod setup;
pub mod sources;
pub mod stats;
pub mod status;
pub mod suggest;
pub mod train;
pub mod watch;

#[cfg(test)]
mod services_migration_tests;

pub use ask::run_ask;
pub use common::start_url_from_cfg;
pub use completions::run_completions;
pub use config::run_config;
pub use crawl::run_crawl;
pub use debug::run_debug;
pub use dedupe::run_dedupe;
pub use doctor::run_doctor;
pub use domains::run_domains;
pub use embed::run_embed;
pub use evaluate::run_evaluate;
pub use extract::run_extract;
pub use ingest::run_ingest;
pub use map::run_map;
pub use mcp::run_mcp;
pub use migrate::run_migrate;
pub use query::run_query;
pub use research::run_research;
pub use retrieve::run_retrieve;
pub use scrape::run_scrape;
pub use screenshot::run_screenshot;
pub use search::run_search;
pub use serve::run_serve;
pub use sessions::run_sessions;
pub use setup::run_setup;
pub use sources::run_sources;
pub use stats::run_stats;
pub use status::run_status;
pub use suggest::run_suggest;
pub use train::run_train;
pub use watch::run_watch;

use crate::core::config::Config;
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
