pub mod ask;
pub mod common;
mod common_jobs;
pub mod common_urls;
pub mod completions;
pub mod crawl;
pub mod debug;
pub mod dedupe;
pub mod doctor;
pub mod domains;
pub mod embed;
pub mod evaluate;
pub mod export;
pub mod extract;
pub mod graph;
pub mod ingest;
pub mod ingest_common;
pub mod job_contracts;
pub mod map;
pub mod mcp;
pub mod migrate;
pub mod probe;
pub mod query;
pub mod refresh;
pub mod research;
pub mod retrieve;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod serve;
pub mod sessions;
pub mod sources;
pub mod stats;
pub mod status;
pub mod suggest;
pub mod watch;

#[cfg(test)]
mod services_migration_tests;

pub use ask::run_ask;
pub use common::start_url_from_cfg;
pub use completions::run_completions;
pub use crawl::run_crawl;
pub use debug::run_debug;
pub use dedupe::run_dedupe;
pub use doctor::run_doctor;
pub use domains::run_domains;
pub use embed::run_embed;
pub use evaluate::run_evaluate;
pub use export::run_export;
pub use extract::run_extract;
pub use graph::run_graph;
pub use ingest::run_ingest;
pub use map::run_map;
pub use mcp::run_mcp;
pub use migrate::run_migrate;
pub use query::run_query;
pub use refresh::run_refresh;
pub use research::run_research;
pub use retrieve::run_retrieve;
pub use scrape::run_scrape;
pub use screenshot::run_screenshot;
pub use search::run_search;
pub use serve::run_serve;
pub use sessions::run_sessions;
pub use sources::run_sources;
pub use stats::run_stats;
pub use status::run_status;
pub use suggest::run_suggest;
pub use watch::run_watch;

use crate::crates::core::config::Config;
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
        .or_else(|| (!cfg.positional.is_empty()).then(|| cfg.positional.join(" ")))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod command_signature_tests {
    use super::*;
    use crate::crates::services::context::ServiceContext;
    use std::error::Error;
    use std::future::Future;
    use std::pin::Pin;

    type CommandFn = for<'a> fn(
        &'a Config,
        &'a ServiceContext,
    )
        -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>>;

    #[allow(dead_code)]
    fn _assert_command_signatures(
        _crawl: CommandFn,
        _embed: CommandFn,
        _extract: CommandFn,
        _ingest: CommandFn,
    ) {
    }

    #[test]
    fn commands_accept_service_context() {
        _assert_command_signatures(run_crawl, run_embed, run_extract, run_ingest);
    }
}
