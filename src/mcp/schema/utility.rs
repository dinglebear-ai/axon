use rmcp::schemars;
use serde::{Deserialize, Serialize};

use super::requests::{McpRenderMode, ResponseMode, SearchTimeRange};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

/// Request parameters for the elicit_demo action.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElicitDemoRequest {
    /// Optional prompt message shown to the user above the elicitation form.
    pub message: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

// ── vertical_scrape ─────────────────────────────────────────────────────────

/// Subaction for the `vertical_scrape` action.
///
/// - `run` — deprecated; returns an error directing callers to `action=scrape`
/// - `list` — return the extractor catalog (id, label, description, url_patterns)
/// - `capabilities` — per-extractor auth_required + rate_limit info
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerticalScrapeSubaction {
    #[default]
    Run,
    List,
    Capabilities,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerticalScrapeRequest {
    /// Which operation to perform (default: run for backward-compatible error handling).
    #[serde(default)]
    pub subaction: VerticalScrapeSubaction,
    /// Extractor name — optional filter for `capabilities`; ignored by `list`.
    /// One of: github_repo, github_release, reddit, pypi, npm, crates_io,
    /// docker_hub, huggingface_model, dev_to, shopify, youtube_video, amazon, ebay.
    /// Use `list` to discover the full catalog.
    pub extractor: Option<String>,
    /// Deprecated with `subaction=run`; use `action=scrape` with this URL instead.
    pub url: Option<String>,
    /// Whether to embed the result into Qdrant after extraction.
    pub embed: Option<bool>,
    /// Qdrant collection to embed into (overrides cfg.collection).
    pub collection: Option<String>,
}
