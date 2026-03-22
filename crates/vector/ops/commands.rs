pub(crate) mod ask;
mod evaluate;
mod query;
pub(crate) mod streaming;
mod suggest;

pub use evaluate::evaluate_payload;
pub use query::query_results;
pub use suggest::discover_crawl_suggestions;

use crate::crates::core::config::Config;

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
