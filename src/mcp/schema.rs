use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AxonRequest {
    Status(StatusRequest),
    Crawl(CrawlRequest),
    Extract(ExtractRequest),
    Embed(EmbedRequest),
    Ingest(IngestRequest),
    Query(QueryRequest),
    Retrieve(RetrieveRequest),
    Search(SearchRequest),
    Map(MapRequest),
    Evaluate(EvaluateRequest),
    Suggest(SuggestRequest),
    Doctor(DoctorRequest),
    Domains(DomainsRequest),
    Sources(SourcesRequest),
    Stats(StatsRequest),
    Help(HelpRequest),
    Artifacts(ArtifactsRequest),
    Scrape(ScrapeRequest),
    Research(ResearchRequest),
    Ask(AskRequest),
    Screenshot(ScreenshotRequest),
    Debug(DebugRequest),
    Dedupe(DedupeRequest),
    Migrate(MigrateRequest),
    Watch(WatchRequest),
    Setup(SetupRequest),
    ElicitDemo(ElicitDemoRequest),
    VerticalScrape(VerticalScrapeRequest),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Path,
    Inline,
    Both,
    #[serde(alias = "auto-inline")]
    AutoInline,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CrawlRequest {
    pub subaction: Option<CrawlSubaction>,
    pub urls: Option<Vec<String>>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub sitemap_since_days: Option<u32>,
    pub render_mode: Option<McpRenderMode>,
    pub delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrawlSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpRenderMode {
    Http,
    Chrome,
    AutoSwitch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpScrapeFormat {
    Markdown,
    Html,
    RawHtml,
    Json,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    pub subaction: Option<ExtractSubaction>,
    pub urls: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub max_pages: Option<u32>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbedRequest {
    pub subaction: Option<EmbedSubaction>,
    pub input: Option<String>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmbedSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestRequest {
    pub subaction: Option<IngestSubaction>,
    pub source_type: Option<IngestSourceType>,
    pub target: Option<String>,
    pub include_source: Option<bool>,
    pub sessions: Option<SessionsIngestOptions>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSourceType {
    Github,
    Reddit,
    Youtube,
    Sessions,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionsIngestOptions {
    pub claude: Option<bool>,
    pub codex: Option<bool>,
    pub gemini: Option<bool>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactsRequest {
    pub subaction: ArtifactsSubaction,
    pub path: Option<String>,
    pub pattern: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Lines of context before/after each grep match (like rg -C N). Default: 0.
    pub context_lines: Option<usize>,
    /// artifacts.read: return full content (paginated). Requires explicit opt-in.
    pub full: Option<bool>,
    /// artifacts.clean: delete files older than this many hours. Required for clean.
    pub max_age_hours: Option<u64>,
    /// artifacts.clean: preview-only mode. Defaults to true — no files deleted unless false.
    pub dry_run: Option<bool>,
    /// Response mode for list/search subactions (path | inline | both). Defaults to path.
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactsSubaction {
    Head,
    Grep,
    Wc,
    Read,
    /// List all artifacts in the artifact directory with metadata.
    List,
    /// Delete a single artifact by path (must be within artifact root).
    Delete,
    /// Bulk-delete artifacts older than max_age_hours. Dry-run by default.
    Clean,
    /// Regex search across all artifact files.
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Qdrant collection to search. Defaults to the server's configured collection.
    pub collection: Option<String>,
    /// Lower bound for temporal filter. Formats: 7d, 30d, YYYY-MM-DD, RFC3339.
    /// Restricts results to content indexed on or after this date.
    pub since: Option<String>,
    /// Upper bound for temporal filter. Same formats as `since`.
    /// Restricts results to content indexed on or before this date.
    pub before: Option<String>,
    /// Per-request hybrid search override. `false` forces dense-only retrieval
    /// (skips BM42 sparse + RRF). When unset, falls back to server config
    /// (`AXON_HYBRID_SEARCH`, default true). Useful for A/B comparison.
    pub hybrid_search: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetrieveRequest {
    pub url: Option<String>,
    pub max_points: Option<usize>,
    /// Qdrant collection to read from. Defaults to the server's configured collection.
    pub collection: Option<String>,
    /// Lower bound for temporal filter. Formats: 7d, 30d, YYYY-MM-DD, RFC3339.
    /// Restricts retrieved chunks to content indexed on or after this date.
    pub since: Option<String>,
    /// Upper bound for temporal filter. Same formats as `since`.
    /// Restricts retrieved chunks to content indexed on or before this date.
    pub before: Option<String>,
    pub response_mode: Option<ResponseMode>,
    /// Opaque cursor for fetching the next slice of document content.
    pub cursor: Option<String>,
    /// Maximum tokens to return in a single response slice. Default: 10,000.
    pub token_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapRequest {
    pub url: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DebugRequest {
    pub context: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DedupeRequest {
    pub collection: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MigrateRequest {
    pub from: Option<String>,
    pub to: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub subaction: Option<WatchSubaction>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub task_type: Option<String>,
    pub task_payload: Option<Value>,
    pub every_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WatchSubaction {
    Create,
    List,
    Get,
    RunNow,
    History,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    pub mode: Option<SetupMode>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SetupMode {
    Check,
    FirstRun,
    Repair,
    MigrateEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    #[serde(alias = "question")]
    pub query: Option<String>,
    /// Include RAG diagnostics in response. Overrides cfg.ask_diagnostics.
    pub diagnostics: Option<bool>,
    /// Compare hybrid RAG against dense-only RAG instead of RAG against baseline.
    pub retrieval_ab: Option<bool>,
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
pub struct SuggestRequest {
    #[serde(alias = "query")]
    pub focus: Option<String>,
    /// Maximum number of suggestions to return. Overrides cfg.search_limit.
    pub limit: Option<usize>,
    /// Qdrant collection to inspect. Defaults to the server's configured collection.
    pub collection: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DoctorRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainsRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourcesRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatsRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScrapeRequest {
    pub url: Option<String>,
    /// Rendering engine override (http | chrome | auto_switch). Overrides cfg.render_mode.
    pub render_mode: Option<McpRenderMode>,
    /// Output format (markdown | html | raw_html | json). Overrides cfg.format.
    pub format: Option<McpScrapeFormat>,
    /// Whether to embed the scraped content into Qdrant. Overrides cfg.embed.
    pub embed: Option<bool>,
    pub response_mode: Option<ResponseMode>,
    /// CSS selector to scope content extraction (e.g. "article, main, .content").
    pub root_selector: Option<String>,
    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads").
    pub exclude_selector: Option<String>,
    /// Opaque cursor for fetching the next slice of document content.
    pub cursor: Option<String>,
    /// Maximum tokens to return in a single response slice. Default: 10,000.
    pub token_budget: Option<usize>,
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub query: Option<String>,
    /// Deprecated compatibility field. `false`/unset is accepted as a no-op;
    /// `true` is rejected by the handler because graph retrieval is not wired.
    pub graph: Option<bool>,
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

impl AskRequest {
    pub fn unsupported_graph_error(&self) -> Option<&'static str> {
        self.graph
            .unwrap_or(false)
            .then_some("graph retrieval is not supported; omit graph or set graph to false")
    }
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

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AxonToolResponse {
    pub ok: bool,
    pub action: String,
    pub subaction: String,
    pub data: Value,
}

impl AxonToolResponse {
    pub fn ok(action: &str, subaction: &str, data: Value) -> Self {
        Self {
            ok: true,
            action: action.to_string(),
            subaction: subaction.to_string(),
            data,
        }
    }
}

// ── vertical_scrape ─────────────────────────────────────────────────────────

/// Subaction for the `vertical_scrape` action.
///
/// - `run` — invoke the named extractor; does NOT fall through to generic scrape
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
    /// Which operation to perform (default: run).
    #[serde(default)]
    pub subaction: VerticalScrapeSubaction,
    /// Extractor name — required for `run` and `capabilities`.
    /// One of: github_repo, github_release, reddit, pypi, npm, crates_io,
    /// docker_hub, huggingface_model, dev_to, shopify, youtube_video, amazon, ebay.
    /// Use `list` to discover the full catalog.
    pub extractor: Option<String>,
    /// URL to extract (required for `run`).
    pub url: Option<String>,
    /// Whether to embed the result into Qdrant after extraction.
    pub embed: Option<bool>,
    /// Qdrant collection to embed into (overrides cfg.collection).
    pub collection: Option<String>,
}

pub fn parse_axon_request(raw: Map<String, Value>) -> Result<AxonRequest, String> {
    serde_json::from_value(Value::Object(raw)).map_err(|e| format!("invalid request shape: {e}"))
}

#[cfg(test)]
#[path = "schema/tests.rs"]
mod tests;
