//! Generic service result types used by query, scrape, system, and other
//! non-entrypoint service modules.

use super::client_server::ArtifactHandle;

// ── Options / pagination ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveOptions {
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub limit: usize,
    pub offset: usize,
    pub time_range: Option<ServiceTimeRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapOptions {
    pub limit: usize,
    pub offset: usize,
}

// ── System / discovery results ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourcesResult {
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    /// Indexed URLs paired with their chunk counts.
    pub urls: Vec<(String, usize)>,
    /// Optional per-schema-version chunk counts (populated only when the
    /// caller opts in via `--by-schema-version`). Implicit pre-`axon_rust-lu6a`
    /// points (no `payload_schema_version` field) are reported under the
    /// key `1`. See `services::system::sources_with_breakdown`.
    pub schema_version_breakdown: Option<std::collections::BTreeMap<u32, usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainSourcesResult {
    pub domain: String,
    pub count: usize,
    pub limit: usize,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainFacet {
    pub domain: String,
    pub vectors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainsResult {
    pub domains: Vec<DomainFacet>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainIndexedResult {
    pub domain: String,
    pub indexed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetailedDomainFacet {
    pub domain: String,
    pub vectors: usize,
    pub urls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetailedDomainsResult {
    pub domains: Vec<DetailedDomainFacet>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatsResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DoctorResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DebugResult {
    pub payload: serde_json::Value,
}

/// True DB-level job counts across all job types.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusTotals {
    pub crawl: i64,
    pub extract: i64,
    pub embed: i64,
    pub ingest: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusResult {
    pub payload: serde_json::Value,
    pub text: String,
    pub totals: StatusTotals,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceJob {
    pub id: uuid::Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub urls_json: Option<serde_json::Value>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: Option<serde_json::Value>,
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reclaimed_reason: Option<String>,
}

// ── From<XJob> for ServiceJob ────────────────────────────────────────────────

impl From<crate::jobs::crawl::CrawlJob> for ServiceJob {
    fn from(job: crate::jobs::crawl::CrawlJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: Some(job.url),
            source_type: None,
            target: None,
            urls_json: None,
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::embed::EmbedJob> for ServiceJob {
    fn from(job: crate::jobs::embed::EmbedJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: None,
            target: Some(job.input_text),
            urls_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::extract::ExtractJob> for ServiceJob {
    fn from(job: crate::jobs::extract::ExtractJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: None,
            target: None,
            urls_json: Some(job.urls_json),
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::ingest::IngestJob> for ServiceJob {
    fn from(job: crate::jobs::ingest::IngestJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: Some(job.source_type),
            target: Some(job.target),
            urls_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

// ── Named constructors ────────────────────────────────────────────────────────

impl ServiceJob {
    pub fn status_enum(&self) -> crate::jobs::status::JobStatus {
        crate::jobs::status::JobStatus::from_str(&self.status)
    }

    pub fn from_status_row(row: crate::jobs::backend::JobStatusRow) -> Self {
        Self {
            id: row.id,
            status: row.status.as_str().to_string(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error_text: row.error_text,
            url: None,
            source_type: None,
            target: None,
            urls_json: None,
            result_json: row.result_json,
            config_json: None,
            attempt_count: row.attempt_count,
            active_attempt_id: row.active_attempt_id,
            last_reclaimed_at: row.last_reclaimed_at,
            last_reclaimed_reason: row.last_reclaimed_reason,
        }
    }

    pub fn from_summary(summary: crate::jobs::backend::JobSummary) -> Self {
        Self {
            id: summary.id,
            status: summary.status.as_str().to_string(),
            created_at: summary.created_at,
            // JobSummary carries no updated_at; use created_at as a floor value.
            updated_at: summary.created_at,
            started_at: None,
            finished_at: None,
            error_text: None,
            url: None,
            source_type: None,
            target: Some(summary.target),
            urls_json: None,
            result_json: None,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DedupeResult {
    pub completed: bool,
    pub duplicate_groups: usize,
    pub deleted: usize,
}

// ── Query / retrieve / ask / evaluate / suggest ──────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryHit {
    pub rank: u64,
    pub score: f64,
    pub rerank_score: f64,
    pub url: String,
    pub source: String,
    pub snippet: String,
    pub chunk_index: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryResult {
    pub results: Vec<QueryHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentBackend {
    /// Content reconstructed from Qdrant vector chunks.
    Qdrant,
    /// Content read from a stored source file (markdown/html).
    StoredSource,
    /// Content fetched from a live scrape refresh.
    LiveScrape,
}

impl std::fmt::Display for DocumentBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Qdrant => write!(f, "qdrant"),
            Self::StoredSource => write!(f, "stored_source"),
            Self::LiveScrape => write!(f, "live_scrape"),
        }
    }
}

/// A slice of document content with windowing/pagination metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PagedDocument {
    /// The document content (markdown).
    pub content: String,
    /// True if the document was truncated to fit the response budget.
    pub truncated: bool,
    /// Conservative estimate of tokens in this slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    /// Opaque cursor for fetching the next slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Conservative estimate of remaining tokens after this slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    /// The backend that provided this content.
    pub backend: DocumentBackend,
}

impl PagedDocument {
    /// Default token budget for inline document reads (10k tokens).
    pub const DEFAULT_TOKEN_BUDGET: usize = 10_000;
    /// Conservative approximation: 1 token ≈ 4 characters.
    pub const CHARS_PER_TOKEN: usize = 4;

    /// Window a full document into a paged slice based on a token budget and cursor.
    pub fn from_full_content(
        full_content: &str,
        cursor: Option<&str>,
        token_budget: Option<usize>,
        backend: DocumentBackend,
    ) -> Self {
        let budget = token_budget.unwrap_or(Self::DEFAULT_TOKEN_BUDGET);
        let char_budget = budget * Self::CHARS_PER_TOKEN;

        let start_offset = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);

        if start_offset >= full_content.len() {
            return Self {
                content: String::new(),
                truncated: false,
                token_estimate: Some(0),
                next_cursor: None,
                remaining_tokens_estimate: Some(0),
                backend,
            };
        }

        // Slice at char boundaries to avoid panics.
        // find_at_char_boundary(full_content, start_offset + char_budget)
        let end_limit = (start_offset + char_budget).min(full_content.len());
        let actual_end = if full_content.is_char_boundary(end_limit) {
            end_limit
        } else {
            // Step back to previous boundary
            let mut pos = end_limit - 1;
            while pos > start_offset && !full_content.is_char_boundary(pos) {
                pos -= 1;
            }
            pos
        };

        let content = full_content[start_offset..actual_end].to_string();
        let truncated = actual_end < full_content.len();

        let token_estimate = content.len().div_ceil(Self::CHARS_PER_TOKEN);
        let remaining_chars = full_content.len() - actual_end;
        let remaining_tokens_estimate = remaining_chars.div_ceil(Self::CHARS_PER_TOKEN);

        let next_cursor = if truncated {
            Some(actual_end.to_string())
        } else {
            None
        };

        Self {
            content,
            truncated,
            token_estimate: Some(token_estimate),
            next_cursor,
            remaining_tokens_estimate: Some(remaining_tokens_estimate),
            backend,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetrieveResult {
    pub chunk_count: usize,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_url: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variant_errors: Vec<ServiceRetrieveVariantError>,

    // Document windowing fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<DocumentBackend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceRetrieveVariantError {
    pub url: String,
    pub error: String,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskDiagnostics {
    pub candidate_pool: usize,
    pub reranked_pool: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_selected: usize,
    pub context_chars: usize,
    #[serde(default)]
    pub full_doc_fetch_skipped: bool,
    #[serde(default)]
    pub full_doc_fetch_skip_reason: String,
    #[serde(default)]
    pub detected_complexity: String,
    #[serde(default)]
    pub resolved_full_docs: usize,
    #[serde(default)]
    pub full_docs_source: String,
    pub min_relevance_score: f64,
    #[serde(default)]
    pub ask_candidate_limit: usize,
    #[serde(default)]
    pub ask_chunk_limit: usize,
    #[serde(default)]
    pub ask_backfill_chunks: usize,
    #[serde(default)]
    pub ask_doc_chunk_limit: usize,
    #[serde(default)]
    pub ask_hybrid_candidates: usize,
    #[serde(default)]
    pub ask_full_docs_configured: usize,
    #[serde(default)]
    pub ask_full_docs_explicit: bool,
    #[serde(default)]
    pub ask_fulldoc_skip_enabled: bool,
    #[serde(default)]
    pub ask_max_context_chars: usize,
    pub doc_fetch_concurrency: usize,
    pub top_domains: Vec<String>,
    pub authority_ratio: f64,
    #[serde(default)]
    pub configured_authority_ratio: f64,
    #[serde(default)]
    pub product_authority_ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus_health: Option<CorpusHealthDiagnostic>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorpusHealthKind {
    Healthy,
    NoRetrievalCandidates,
    ThinDomain,
    RetrievedNotSelected,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CorpusHealthDiagnostic {
    pub kind: CorpusHealthKind,
    pub reason: String,
    pub selected_domain_count: usize,
    pub top_domain_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainMode {
    ExplainOnly,
    ExplainWithAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainScoreKind {
    Cosine,
    Rrf,
    NamedDense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainScoreComponentStatus {
    Applied,
    Skipped,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainScoreComponent {
    pub name: String,
    pub value: f64,
    pub status: AskExplainScoreComponentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainRetrieval {
    pub query: String,
    pub keyword_query: String,
    pub dual_search: bool,
    pub collection: String,
    pub candidate_limit: usize,
    pub hybrid_search_enabled: bool,
    pub hybrid_candidate_limit: usize,
    pub score_kind: AskExplainScoreKind,
    pub vector_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_query_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainFilterDecisionKind {
    Kept,
    DroppedLowSignal,
    DroppedMinRelevance,
    DroppedTopicalOverlap,
    DroppedDuplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainFilterDecision {
    pub kind: AskExplainFilterDecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainSelectionDecisionKind {
    SelectedTopChunk,
    PlannedFullDoc,
    InsertedFullDoc,
    SkippedPlannedFullDoc,
    SkippedFullDocFetchSkipped,
    SelectedSupplemental,
    SkippedBudget,
    NotSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainSelectionDecision {
    pub kind: AskExplainSelectionDecisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskExplainInsertionMode {
    TopChunk,
    PlannedFullDoc,
    InsertedFullDoc,
    Supplemental,
    NotSelected,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainCandidate {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_rerank_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_full_doc_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_context_rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insertion_mode: Option<AskExplainInsertionMode>,
    pub retrieval_score: f64,
    pub rerank_score: f64,
    pub score_kind: AskExplainScoreKind,
    pub score_components: Vec<AskExplainScoreComponent>,
    pub filter_decisions: Vec<AskExplainFilterDecision>,
    pub selection_decisions: Vec<AskExplainSelectionDecision>,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainContextSource {
    pub source_id: String,
    pub url: String,
    pub tier: String,
    #[serde(default)]
    pub sort_rank: usize,
    #[serde(default)]
    pub sort_score: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainContext {
    pub planned_full_doc_urls: Vec<String>,
    pub full_doc_fetch_skipped: bool,
    pub full_doc_fetch_skip_reason: String,
    pub full_doc_fetch_mode: String,
    pub final_source_order: Vec<AskExplainContextSource>,
    pub context_char_budget: usize,
    pub context_chars_used: usize,
    pub truncated_by_budget: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskExplainTrace {
    pub mode: AskExplainMode,
    pub retrieval: AskExplainRetrieval,
    pub candidates: Vec<AskExplainCandidate>,
    pub context: AskExplainContext,
    pub candidate_trace_limit: usize,
    pub candidate_trace_truncated: bool,
    pub llm_skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AskTiming {
    pub retrieval: u128,
    pub context_build: u128,
    pub llm: u128,
    pub total: u128,
    // Populated only when ask_diagnostics=true; otherwise omitted via skip_serializing_if.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tei_embed_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qdrant_primary_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qdrant_secondary_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_select_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_doc_fetch_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplemental_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_ttft_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_total_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streamed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskResult {
    pub query: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub diagnostics: Option<AskDiagnostics>,
    #[serde(default)]
    pub explain: Option<AskExplainTrace>,
    pub timing_ms: AskTiming,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateDiagnostics {
    pub candidate_pool: usize,
    pub reranked_pool: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_selected: usize,
    pub context_chars: usize,
    pub min_relevance_score: f64,
    pub doc_fetch_concurrency: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateTiming {
    pub retrieval: u128,
    pub context_build: u128,
    pub rag_llm: u128,
    pub baseline_llm: u128,
    pub research_elapsed_ms: u128,
    pub analysis_llm_ms: u128,
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateCrawlEnqueueOutcome {
    pub url: String,
    pub job_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvaluateResult {
    pub query: String,
    pub rag_answer: String,
    pub baseline_answer: String,
    pub analysis_answer: String,
    pub source_urls: Vec<String>,
    pub crawl_suggestions: Vec<Suggestion>,
    pub crawl_enqueue_outcomes: Vec<EvaluateCrawlEnqueueOutcome>,
    pub ref_chunk_count: usize,
    pub diagnostics: Option<EvaluateDiagnostics>,
    pub timing_ms: EvaluateTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Suggestion {
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SuggestResult {
    pub suggestions: Vec<Suggestion>,
}

// ── Scrape / map / search / research ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScrapeResult {
    pub payload: serde_json::Value,
    pub url: String,
    pub markdown: String,
    pub output: String,
    pub artifact_handle: Option<ArtifactHandle>,

    // Document windowing fields
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<DocumentBackend>,
    /// URLs the extractor recommends crawling as a follow-up (e.g. docs.rs for
    /// a crates.io crate). Empty for generic scrapes and most verticals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata (from `ScrapedDoc.extra`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    /// Vertical extractor name (from `ScrapedDoc.extractor_name`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extractor_name: Option<String>,
    /// Page title from the vertical extractor. None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeDocument {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub content_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeTiming {
    pub scrape: u128,
    pub llm: u128,
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeResult {
    pub urls: Vec<String>,
    pub documents: Vec<SummarizeDocument>,
    pub summary: String,
    pub context_chars: usize,
    pub context_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<SummarizeUsage>,
    pub timing_ms: SummarizeTiming,
}

/// Typed result of a `map` (URL discovery) operation.
///
/// Replaces the previous `MapResult { payload: serde_json::Value }` pass-through.
/// Serializes to the same JSON shape so callers that use `serde_json::to_value`
/// remain wire-compatible.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapResult {
    /// The start URL that was mapped.
    pub url: String,
    /// Number of URLs returned in this page slice (equals `urls.len()`).
    ///
    /// This is the post-pagination count. For the pre-pagination total
    /// across all discovered URLs, use [`MapResult::total`]. The wire JSON
    /// key remains `mapped_urls` for backward compatibility.
    #[serde(rename = "mapped_urls")]
    pub returned_url_count: u64,
    /// Pre-pagination total URL count (all discovered URLs before offset/limit).
    ///
    /// CLI callers always pass `limit=0, offset=0`, so `total == mapped_urls` there.
    /// MCP callers use this field for `total_urls` in the response to avoid
    /// reporting the paginated count as the total.
    pub total: u64,
    /// Raw count of URLs found in sitemap.xml (before dedup).
    pub sitemap_urls: usize,
    /// Number of pages actually fetched during a crawl pass (0 in sitemap-only mode).
    pub pages_seen: u32,
    /// Pages below the minimum markdown character threshold.
    pub thin_pages: u32,
    /// Wall-clock time for the entire map operation.
    pub elapsed_ms: u64,
    /// How the URLs were discovered: `"sitemap"`, `"crawl"`, or `"bounded-structure"`.
    pub map_source: String,
    /// Optional user-visible warning (e.g. too few URLs found).
    pub warning: Option<String>,
    /// The discovered URLs (deduplicated, sorted), after offset/limit pagination.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub results: Vec<serde_json::Value>,
}

/// Origin of the synthesized summary returned by `research`.
///
/// `Llm` means the LLM produced the summary; `Fallback` means synthesis
/// failed and a deterministic snippet-based summary was substituted;
/// `None` means no extractions were available to summarize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SummarySource {
    Llm,
    Fallback,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchHit {
    pub position: usize,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchExtraction {
    pub url: String,
    pub title: String,
    pub extracted: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ResearchUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchTiming {
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchPayload {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
    pub search_results: Vec<ResearchHit>,
    pub extractions: Vec<ResearchExtraction>,
    pub summary: Option<String>,
    pub summary_source: SummarySource,
    pub usage: ResearchUsage,
    pub timing_ms: ResearchTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchResult {
    pub payload: ResearchPayload,
}

// ── Lifecycle: crawl / embed / extract ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CrawlStartJob {
    pub job_id: String,
    pub url: String,
    pub output_dir: String,
    pub predicted_paths: Vec<String>,
    pub predicted_artifact_handles: Vec<ArtifactHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CrawlStartResult {
    pub job_ids: Vec<String>,
    pub output_dir: Option<String>,
    pub predicted_paths: Vec<String>,
    pub predicted_artifact_handles: Vec<ArtifactHandle>,
    pub jobs: Vec<CrawlStartJob>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CrawlJobResult {
    pub payload: serde_json::Value,
    pub output_files: Option<Vec<String>>,
    pub output_file_handles: Vec<ArtifactHandle>,
}

/// Result of a synchronous (--wait true) crawl, including all phases
/// (HTTP crawl, Chrome fallback, sitemap backfill, embed, audit diff).
#[derive(Debug, Clone, PartialEq)]
pub struct CrawlSyncResult {
    pub pages_seen: u32,
    pub markdown_files: u32,
    pub thin_pages: u32,
    pub error_pages: u32,
    pub waf_blocked_pages: u32,
    pub waf_diagnostics: Option<crate::crawl::engine::WafDiagnostics>,
    pub elapsed_ms: u128,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmbedStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmbedJobResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtractStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExtractJobResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractSyncResult {
    pub summary: serde_json::Value,
    pub summary_path: String,
    pub items_path: String,
    pub total_items: usize,
    pub duration_ms: u128,
}

// ── Migrate ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateResult {
    pub from: String,
    pub to: String,
    pub points_migrated: u64,
    pub pages_processed: u64,
}

// ── Ingest / screenshot ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IngestResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IngestStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IngestJobResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotResult {
    pub url: String,
    pub path: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_handle: Option<ArtifactHandle>,
}

// ── Job list pagination ──────────────────────────────────────────────────

/// Paginated job list result — always includes true DB total count.
#[derive(Debug, Clone, PartialEq)]
pub struct JobListResult<T> {
    /// The fetched slice of jobs (up to `limit` items).
    pub jobs: Vec<T>,
    /// True total number of jobs in the DB (may exceed `jobs.len()`).
    pub total: i64,
    /// The limit that was applied.
    pub limit: i64,
    /// The offset that was applied.
    pub offset: i64,
}

impl<T> JobListResult<T> {
    pub fn new(jobs: Vec<T>, total: i64, limit: i64, offset: i64) -> Self {
        Self {
            jobs,
            total,
            limit,
            offset,
        }
    }

    /// True if the displayed slice is a subset of all available jobs.
    pub fn is_truncated(&self) -> bool {
        self.offset + self.limit < self.total
    }
}

// ── diff ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Same,
    Changed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkEntry {
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffResult {
    pub url_a: String,
    pub url_b: String,
    pub status: DiffStatus,
    /// Unified diff of the markdown content, if any changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_diff: Option<String>,
    pub metadata_changes: Vec<MetadataChange>,
    pub links_added: Vec<LinkEntry>,
    pub links_removed: Vec<LinkEntry>,
    pub word_count_delta: i64,
}

// ── brand ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorUsage {
    Primary,
    Secondary,
    Background,
    Text,
    Accent,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrandColor {
    pub hex: String,
    pub usage: ColorUsage,
    pub count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogoVariant {
    pub url: String,
    /// "favicon" | "apple-touch-icon" | "logo" | "og-image" | "svg"
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrandResult {
    pub url: String,
    pub name: Option<String>,
    pub colors: Vec<BrandColor>,
    pub fonts: Vec<String>,
    pub logos: Vec<LogoVariant>,
    pub logo_url: Option<String>,
    pub favicon_url: Option<String>,
    pub og_image: Option<String>,
}
