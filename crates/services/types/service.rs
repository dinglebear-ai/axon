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

#[derive(Debug, Clone, PartialEq)]
pub struct StatusResult {
    pub payload: serde_json::Value,
    pub text: String,
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

// ── Graph ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GraphBuildResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatusResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphExploreResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatsResult {
    pub payload: serde_json::Value,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshStartResult {
    pub job_id: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefreshJobResult {
    pub job: Option<crate::crates::jobs::refresh::RefreshJob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefreshJobListResult {
    pub jobs: Vec<crate::crates::jobs::refresh::RefreshJob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefreshRunResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenshotResult {
    pub payload: serde_json::Value,
}
