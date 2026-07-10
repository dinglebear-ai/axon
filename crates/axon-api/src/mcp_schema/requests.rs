use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::source::{
    JobKind, JobRetryMode, LifecycleStatus, MetadataMap, PipelinePhase, Severity, SourceId,
    Timestamp, Visibility, WatchId,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
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
    pub max_sitemaps: Option<usize>,
    pub sitemap_since_days: Option<u32>,
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobsRequest {
    pub subaction: Option<JobsSubaction>,
    pub job_id: Option<String>,
    pub status: Option<LifecycleStatus>,
    pub kind: Option<JobKind>,
    pub source_id: Option<SourceId>,
    pub watch_id: Option<WatchId>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub after_sequence: Option<u64>,
    pub since_sequence: Option<u64>,
    pub severity: Option<Severity>,
    pub visibility: Option<Visibility>,
    pub reason: Option<String>,
    pub retry_mode: Option<JobRetryMode>,
    pub from_phase: Option<PipelinePhase>,
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub overrides: MetadataMap,
    pub stale_before: Option<Timestamp>,
    pub older_than: Option<Timestamp>,
    pub dry_run: Option<bool>,
    pub confirm: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobsSubaction {
    List,
    Get,
    Status,
    Events,
    Stream,
    Cancel,
    Retry,
    Recover,
    Cleanup,
    Clear,
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
    Llm,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    pub subaction: Option<ExtractSubaction>,
    pub urls: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<McpRenderMode>,
    pub embed: Option<bool>,
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
    pub source_type: Option<String>,
    /// Qdrant collection to write to. Defaults to the server's configured collection.
    pub collection: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub subaction: Option<MemorySubaction>,
    pub id: Option<String>,
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub edge_type: Option<MemoryEdgeType>,
    pub memory_type: Option<MemoryNodeType>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub query: Option<String>,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub file: Option<String>,
    pub status: Option<String>,
    pub confidence: Option<f64>,
    pub limit: Option<usize>,
    pub depth: Option<usize>,
    pub token_budget: Option<usize>,
    pub response_mode: Option<ResponseMode>,
    /// `reinforce` positive-use signal strength.
    pub amount: Option<f64>,
    /// `pin` — pin (`true`, default) or unpin (`false`).
    pub pinned: Option<bool>,
    /// Free-text reason recorded on `archive`/`forget`/`pin` history events.
    pub reason: Option<String>,
    /// `compact` source memory ids to merge.
    pub memory_ids: Option<Vec<String>>,
    /// `compact` distillation strategy. Only `"concatenate"` is implemented.
    pub strategy: Option<String>,
    /// `compact` — archive the source memories once merged.
    pub archive_sources: Option<bool>,
    /// `import` — records to bulk-import.
    pub records: Option<Vec<crate::source::MemoryRecord>>,
    /// `import` — how to reconcile with existing memories.
    pub import_mode: Option<crate::source::MemoryImportMode>,
    /// `import` — preview the plan without writing.
    pub dry_run: Option<bool>,
    /// `export` — scope to export (all scopes when unset).
    pub export_scope: Option<crate::source::MemoryScope>,
    /// `export` — include archived memories in the export.
    pub include_archived: Option<bool>,
    /// `export` — include `working`-status memories in the export (excluded
    /// by default per contract "Type rules").
    pub include_working: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySubaction {
    Remember,
    List,
    Search,
    Show,
    Link,
    Supersede,
    Context,
    Reinforce,
    Contradict,
    Pin,
    Archive,
    Forget,
    Review,
    Compact,
    Import,
    Export,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNodeType {
    Decision,
    Fact,
    Preference,
    Task,
    Bug,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEdgeType {
    RelatesTo,
    Supersedes,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IngestSourceType {
    Github,
    Gitlab,
    Gitea,
    Git,
    Reddit,
    Youtube,
    Rss,
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
pub struct CodeSearchRequest {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Absolute or process-relative working directory inside an allowed git checkout.
    /// Required for MCP; it must resolve below AXON_CODE_SEARCH_ALLOWED_ROOTS.
    pub cwd: Option<String>,
    /// Repository-relative path prefix to search, such as `src/vector`.
    pub path_prefix: Option<String>,
    /// Search the existing index without refreshing changed local files first.
    pub no_freshness: Option<bool>,
    /// Qdrant collection to search. Defaults to the server's configured collection.
    pub collection: Option<String>,
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
pub struct EndpointsRequest {
    pub url: Option<String>,
    pub include_bundles: Option<bool>,
    pub first_party_only: Option<bool>,
    pub unique_only: Option<bool>,
    pub max_scripts: Option<usize>,
    pub max_scan_bytes: Option<usize>,
    pub verify: Option<bool>,
    pub capture_network: Option<bool>,
    pub probe_rpc: Option<bool>,
    pub probe_rpc_subdomains: Option<bool>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PurgeRequest {
    /// URL (or seed-URL/origin when `prefix` is set) to delete from the index.
    /// **Handler-required despite the `Option`:** the type is `Option<String>`
    /// only so a missing field deserializes to a clean "target is required"
    /// error instead of a serde rejection; `handle_purge` returns an error when
    /// it is `None`. It is not an optional argument.
    pub target: Option<String>,
    /// Match `target` as a prefix over a whole docs subtree / origin.
    #[serde(default)]
    pub prefix: bool,
    /// Preview only — count matches without deleting. **Defaults to `true`** for
    /// agent safety: a bare `purge` previews; set `dry_run=false` to delete.
    pub dry_run: Option<bool>,
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
    Exec,
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
    pub domain: Option<String>,
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
    pub domain: Option<String>,
    pub cursor: Option<String>,
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
pub struct DiffRequest {
    /// First (baseline) URL (required)
    pub url_a: String,
    /// Second (comparison) URL (required)
    pub url_b: String,
    /// Rendering engine override (http | chrome | auto_switch)
    pub render_mode: Option<McpRenderMode>,
    pub response_mode: Option<ResponseMode>,
}
