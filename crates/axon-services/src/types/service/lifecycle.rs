use axon_api::source::{ArtifactId, SourceWarning, Timestamp};
// Lifecycle: extract.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtractStartResult {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExtractJobResult {
    pub payload: serde_json::Value,
}

pub use axon_api::job_dto::ExtractSyncResult;

// ── Migrate ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateResult {
    pub from: String,
    pub to: String,
    pub points_migrated: u64,
    pub pages_processed: u64,
}

// ── Screenshot ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ScreenshotResult {
    pub artifact_id: ArtifactId,
    pub width: u32,
    pub height: u32,
    pub captured_at: Timestamp,
    pub warnings: Vec<SourceWarning>,
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
