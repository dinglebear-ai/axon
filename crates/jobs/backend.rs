use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::crates::jobs::status::JobStatus;

const WAIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

pub type JobId = Uuid;

/// Which job table a job belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Crawl,
    Embed,
    Extract,
    Ingest,
    Refresh,
    Graph,
}

impl JobKind {
    pub fn table_name(self) -> &'static str {
        match self {
            Self::Crawl => "axon_crawl_jobs",
            Self::Embed => "axon_embed_jobs",
            Self::Extract => "axon_extract_jobs",
            Self::Ingest => "axon_ingest_jobs",
            Self::Refresh => "axon_refresh_jobs",
            Self::Graph => "axon_graph_jobs",
        }
    }

    pub fn all() -> &'static [JobKind] {
        &[
            Self::Crawl,
            Self::Embed,
            Self::Extract,
            Self::Ingest,
            Self::Refresh,
            Self::Graph,
        ]
    }
}

/// Job submission payload — one variant per job type.
#[derive(Debug, Clone)]
pub enum JobPayload {
    Crawl {
        url: String,
        config_json: String,
    },
    Embed {
        input: String,
        config_json: String,
    },
    Extract {
        urls: Vec<String>,
        config_json: String,
    },
    Ingest {
        target: String,
        source_type: String,
        config_json: String,
    },
    Refresh {
        url: String,
        config_json: String,
    },
    Graph {
        config_json: String,
    },
}

impl JobPayload {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::Crawl { .. } => JobKind::Crawl,
            Self::Embed { .. } => JobKind::Embed,
            Self::Extract { .. } => JobKind::Extract,
            Self::Ingest { .. } => JobKind::Ingest,
            Self::Refresh { .. } => JobKind::Refresh,
            Self::Graph { .. } => JobKind::Graph,
        }
    }
}

/// A full job row returned from status queries.
#[derive(Debug, Clone)]
pub struct JobStatusRow {
    pub id: JobId,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
}

/// Lightweight summary for list views.
#[derive(Debug, Clone)]
pub struct JobSummary {
    pub id: JobId,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub target: String,
}

pub type BackendResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Low-level job persistence interface implemented by [`FullBackend`] and [`LiteBackend`].
///
/// **Note:** The canonical abstraction consumed by all callers (CLI, MCP, web) is
/// [`ServiceJobRuntime`](crate::crates::services::runtime::ServiceJobRuntime), which
/// returns the richer [`ServiceJob`](crate::crates::services::types::ServiceJob) type.
/// In practice, only `enqueue`, `wait_for_job`, and `job_errors` are delegated through
/// this trait by the service runtime layer; the remaining methods (`list_jobs`,
/// `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs`) are bypassed —
/// `FullServiceRuntime` calls raw Postgres query functions directly, and
/// `LiteServiceRuntime` calls `lite_query::*` directly, to avoid lossy type mapping
/// from `JobStatusRow`/`JobSummary` to `ServiceJob`.
#[async_trait]
pub trait JobBackend: Send + Sync {
    /// Submit a new job. Returns the assigned JobId.
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId>;

    /// Fetch full status row for a job. Returns None if job not found.
    async fn job_status(&self, id: JobId, kind: JobKind) -> BackendResult<Option<JobStatusRow>>;

    /// Cancel a pending or running job. Returns true if canceled, false if not found or already terminal.
    async fn cancel_job(&self, id: JobId, kind: JobKind) -> BackendResult<bool>;

    /// List all jobs of a given type (summary view).
    async fn list_jobs(&self, kind: JobKind) -> BackendResult<Vec<JobSummary>>;

    /// Delete completed + failed jobs older than 24h. Returns count deleted.
    async fn cleanup_jobs(&self, kind: JobKind) -> BackendResult<u64>;

    /// Delete ALL jobs of a given type. Returns count deleted.
    async fn clear_jobs(&self, kind: JobKind) -> BackendResult<u64>;

    /// Get the error_text for a failed job. Returns None if not found or no error.
    async fn job_errors(&self, id: JobId, kind: JobKind) -> BackendResult<Option<String>>;

    /// Poll until the job reaches a terminal state (completed/failed/canceled).
    /// Returns the final status string. Used in lite mode to keep the process
    /// alive while in-process workers finish.
    ///
    /// Times out after `AXON_JOB_WAIT_TIMEOUT_SECS` (default 300s).
    async fn wait_for_job(&self, id: JobId, kind: JobKind) -> BackendResult<String> {
        let timeout_secs: u64 = std::env::var("AXON_JOB_WAIT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

        loop {
            match self.job_status(id, kind).await? {
                Some(row) => match row.status {
                    JobStatus::Completed | JobStatus::Failed | JobStatus::Canceled => {
                        return Ok(row.status.as_str().to_string());
                    }
                    _ => {}
                },
                None => return Err(format!("job {id} not found in {}", kind.table_name()).into()),
            }
            tokio::time::sleep(WAIT_POLL_INTERVAL).await;
            if tokio::time::Instant::now() >= deadline {
                return Err(
                    format!("job {id} timed out after {timeout_secs}s in state running").into(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Compile-time test: verify Arc<dyn JobBackend> is object-safe
    fn _assert_object_safe(_: Arc<dyn JobBackend>) {}
}
