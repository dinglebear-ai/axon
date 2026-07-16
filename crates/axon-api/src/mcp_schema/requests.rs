use serde::{Deserialize, Serialize};

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
    AutoInline,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    pub subaction: Option<ExtractSubaction>,
    pub urls: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<McpRenderMode>,
    pub embed: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractSubaction {
    Start,
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
pub enum SearchTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
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
pub struct MigrateRequest {
    pub from: Option<String>,
    pub to: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

#[path = "requests/watch.rs"]
mod watch;
pub use watch::{WatchRequest, WatchSubaction};

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
    pub response_mode: Option<ResponseMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainsRequest {
    pub domain: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourcesRequest {
    pub domain: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatsRequest {
    pub response_mode: Option<ResponseMode>,
}

#[path = "requests/discovery.rs"]
mod discovery;
pub use discovery::{CapabilitiesRequest, ProvidersRequest, ResolveRequest};

#[path = "requests/graph.rs"]
mod graph;
pub use graph::{GraphDirectionArg, GraphRequest, GraphSubaction};

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
