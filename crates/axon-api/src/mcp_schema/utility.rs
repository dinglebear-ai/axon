use serde::{Deserialize, Serialize};

use super::requests::{McpRenderMode, ResponseMode, SearchTimeRange};
use crate::source::SourceScope;

/// Request parameters for the unified `source` action.
///
/// `source` is the single indexing entrypoint. The handler maps this onto
/// [`crate::source::SourceRequest`] and calls `axon_services::index_source`,
/// which classifies the input (local path, git URL, feed URL, youtube/reddit
/// target, web URL, session selector, or registry target), acquires it, and
/// indexes it through the unified pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    /// Source input string: a local path, git repository URL, feed URL,
    /// youtube target, reddit target, web URL, session selector
    /// (`session:<claude|codex|gemini>:<path>`), or registry target
    /// (`pkg:<npm|pypi|crates>/<package>`).
    pub source: Option<String>,
    /// Optional acquisition scope override (e.g. `page`, `site`, `repo`).
    /// When omitted, the classified family's default scope is used.
    pub scope: Option<SourceScope>,
    /// Qdrant collection to index into. Defaults to the server's configured collection.
    pub collection: Option<String>,
    pub response_mode: Option<ResponseMode>,
    /// Run source indexing as a detached background `JobKind::Source` job
    /// instead of blocking until it completes. When `true`, the response
    /// carries a poll descriptor (`job_id`/`status`/`poll_after_ms`) instead
    /// of the final `SourceResult`. Defaults to `false` (synchronous,
    /// matching prior behavior).
    #[serde(default)]
    pub detached: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub query: Option<String>,
    /// Include RAG diagnostics in response. Overrides cfg.ask_diagnostics.
    pub diagnostics: Option<bool>,
    /// Include per-candidate explain trace and skip LLM synthesis.
    pub explain: Option<bool>,
    /// Qdrant collection to search. Defaults to the server's configured collection.
    pub collection: Option<String>,
    /// Lower bound for temporal filter. Formats: 7d, 30d, YYYY-MM-DD, RFC3339.
    /// Restricts results to content indexed on or after this date.
    pub since: Option<String>,
    /// Upper bound for temporal filter. Same formats as `since`.
    /// Restricts results to content indexed on or before this date.
    pub before: Option<String>,
    /// Per-request hybrid search override. `false` forces dense-only retrieval
    /// (skips BM42 sparse + RRF). When unset, falls back to server config.
    pub hybrid_search: Option<bool>,
    pub ask_chunk_limit: Option<usize>,
    pub ask_full_docs: Option<usize>,
    pub ask_max_context_chars: Option<usize>,
    pub ask_hybrid_candidates: Option<usize>,
    pub ask_min_relevance_score: Option<f64>,
    pub ask_doc_chunk_limit: Option<usize>,
    pub ask_doc_fetch_concurrency: Option<usize>,
    pub ask_backfill_chunks: Option<usize>,
    pub ask_candidate_limit: Option<usize>,
    pub ask_min_citations_nontrivial: Option<usize>,
    pub ask_authoritative_domains: Option<Vec<String>>,
    pub ask_authoritative_boost: Option<f64>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SummarizeRequest {
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    /// Rendering engine override (http | chrome | auto_switch). Overrides cfg.render_mode.
    pub render_mode: Option<McpRenderMode>,
    /// CSS selector to scope content extraction before summarization.
    pub root_selector: Option<String>,
    /// CSS selector to exclude elements before summarization.
    pub exclude_selector: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotRequest {
    pub url: Option<String>,
    pub full_page: Option<bool>,
    pub viewport: Option<String>,
    pub output: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandRequest {
    /// URL to analyze (required)
    pub url: String,
    /// Rendering engine override. Currently accepted but not applied — brand uses
    /// a direct HTTP fetch and ignores the render_mode field. Reserved for a
    /// future Chrome-backed extraction path.
    pub render_mode: Option<McpRenderMode>,
    pub response_mode: Option<ResponseMode>,
}
