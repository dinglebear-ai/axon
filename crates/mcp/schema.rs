use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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
    Refresh(RefreshRequest),
    Graph(GraphRequest),
    Export(ExportRequest),
    ElicitDemo(ElicitDemoRequest),
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Path,
    Inline,
    Both,
    #[serde(alias = "auto-inline")]
    AutoInline,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpRenderMode {
    Http,
    Chrome,
    AutoSwitch,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpScrapeFormat {
    Markdown,
    Html,
    RawHtml,
    Json,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbedRequest {
    pub subaction: Option<EmbedSubaction>,
    pub input: Option<String>,
    pub job_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSourceType {
    Github,
    Reddit,
    Youtube,
    Sessions,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionsIngestOptions {
    pub claude: Option<bool>,
    pub codex: Option<bool>,
    pub gemini: Option<bool>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetrieveRequest {
    pub url: Option<String>,
    pub max_points: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapRequest {
    pub url: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DoctorRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainsRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourcesRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[allow(dead_code)] // accepted for API compat but response is always inline
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatsRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_time_range: Option<SearchTimeRange>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub query: Option<String>,
    /// Enable graph-enhanced retrieval (requires Neo4j). Overrides cfg.ask_graph.
    pub graph: Option<bool>,
    /// Include RAG diagnostics in response. Overrides cfg.ask_diagnostics.
    pub diagnostics: Option<bool>,
    /// Qdrant collection to search. Defaults to the server's configured collection.
    pub collection: Option<String>,
    /// Lower bound for temporal filter. Formats: 7d, 30d, YYYY-MM-DD, RFC3339.
    /// Restricts results to content indexed on or after this date.
    pub since: Option<String>,
    /// Upper bound for temporal filter. Same formats as `since`.
    /// Restricts results to content indexed on or before this date.
    pub before: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotRequest {
    pub url: Option<String>,
    pub full_page: Option<bool>,
    pub viewport: Option<String>,
    pub output: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefreshSubaction {
    Start,
    Status,
    Cancel,
    List,
    Cleanup,
    Clear,
    Recover,
    Schedule,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    pub subaction: Option<RefreshSubaction>,
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub job_id: Option<String>,
    pub schedule_subaction: Option<String>,
    pub schedule_name: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<usize>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphSubaction {
    Build,
    Status,
    Explore,
    Stats,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRequest {
    pub subaction: GraphSubaction,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub all: Option<bool>,
    pub entity: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub include_history: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

/// Request parameters for the elicit_demo action.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
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

pub fn parse_axon_request(raw: Map<String, Value>) -> Result<AxonRequest, String> {
    serde_json::from_value(Value::Object(raw)).map_err(|e| format!("invalid request shape: {e}"))
}

#[cfg(test)]
#[path = "schema/tests.rs"]
mod tests;
