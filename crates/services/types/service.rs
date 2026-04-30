//! Generic service result types used by query, scrape, system, and other
//! non-ACP service modules.

// ── Options / pagination ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrieveOptions {
    pub max_points: Option<usize>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcesResult {
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    /// Indexed URLs paired with their chunk counts.
    pub urls: Vec<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainFacet {
    pub domain: String,
    pub vectors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainsResult {
    pub domains: Vec<DomainFacet>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailedDomainFacet {
    pub domain: String,
    pub vectors: usize,
    pub urls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailedDomainsResult {
    pub domains: Vec<DetailedDomainFacet>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatsResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugResult {
    pub payload: serde_json::Value,
}

/// True DB-level job counts across all job types.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StatusTotals {
    pub crawl: i64,
    pub extract: i64,
    pub embed: i64,
    pub ingest: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatusResult {
    pub payload: serde_json::Value,
    pub text: String,
    pub totals: StatusTotals,
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
}

// ── From<XJob> for ServiceJob ────────────────────────────────────────────────

impl From<crate::crates::jobs::crawl::CrawlJob> for ServiceJob {
    fn from(job: crate::crates::jobs::crawl::CrawlJob) -> Self {
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
        }
    }
}

impl From<crate::crates::jobs::embed::EmbedJob> for ServiceJob {
    fn from(job: crate::crates::jobs::embed::EmbedJob) -> Self {
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
        }
    }
}

impl From<crate::crates::jobs::extract::ExtractJob> for ServiceJob {
    fn from(job: crate::crates::jobs::extract::ExtractJob) -> Self {
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
        }
    }
}

impl From<crate::crates::jobs::ingest::IngestJob> for ServiceJob {
    fn from(job: crate::crates::jobs::ingest::IngestJob) -> Self {
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
        }
    }
}

// ── Named constructors ────────────────────────────────────────────────────────

impl ServiceJob {
    pub fn from_status_row(row: crate::crates::jobs::backend::JobStatusRow) -> Self {
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
        }
    }

    pub fn from_summary(summary: crate::crates::jobs::backend::JobSummary) -> Self {
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DedupeResult {
    pub completed: bool,
    pub duplicate_groups: usize,
    pub deleted: usize,
}

// ── Query / retrieve / ask / evaluate / suggest ──────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrieveResult {
    pub chunks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestResult {
    pub urls: Vec<String>,
}

// ── Scrape / map / search / research ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ScrapeResult {
    pub payload: serde_json::Value,
    pub url: String,
    pub markdown: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResearchResult {
    pub payload: serde_json::Value,
}

// ── Lifecycle: crawl / embed / extract ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlStartJob {
    pub job_id: String,
    pub url: String,
    pub output_dir: String,
    pub predicted_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlStartResult {
    pub job_ids: Vec<String>,
    pub output_dir: Option<String>,
    pub predicted_paths: Vec<String>,
    pub jobs: Vec<CrawlStartJob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrawlJobResult {
    pub payload: serde_json::Value,
    pub output_files: Option<Vec<String>>,
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
    pub waf_diagnostics: Option<crate::crates::crawl::engine::WafDiagnostics>,
    pub elapsed_ms: u128,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedJobResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct IngestResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IngestJobResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenshotResult {
    pub payload: serde_json::Value,
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
