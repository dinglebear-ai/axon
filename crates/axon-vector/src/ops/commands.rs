pub mod ask;
mod code_search;
mod evaluate;
mod query;
mod retrieval;
pub mod streaming;
mod suggest;

pub use ask::{ask_payload, ask_payload_with_deltas};
pub use code_search::{CodeSearchVectorRequest, code_search_hits};
pub use evaluate::{evaluate_payload, evaluate_result, evaluate_result_with_context};
pub use query::{query_hits, query_results};
pub use suggest::discover_crawl_suggestions;

use axon_core::config::Config;

/// Resolve query text from `--query` flag or positional args, trimming whitespace.
/// Returns `None` if both are empty/whitespace-only.
fn resolve_query_text(cfg: &Config) -> Option<String> {
    cfg.query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(ToString::to_string)
        .or_else(|| (!cfg.positional.is_empty()).then(|| cfg.positional.join(" ")))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
