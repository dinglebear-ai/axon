use crate::services::types::client_server::ArtifactHandle;
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
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
            total: total.max(0),
            limit: limit.max(0),
            offset: offset.max(0),
        }
    }

    /// True if the displayed slice is a subset of all available jobs.
    pub fn is_truncated(&self) -> bool {
        self.offset.saturating_add(self.limit) < self.total
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
