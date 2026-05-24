use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::jobs::status::JobStatus;

const WAIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// Stringify any `Display` error into a `Box<dyn Error + Send + Sync>`.
/// Used throughout `jobs/` and `services/` to satisfy `BackendResult` bounds
/// when calling older functions that return `Box<dyn Error>` without Send+Sync.
pub(crate) fn lift_err<E: std::fmt::Display>(e: E) -> Box<dyn std::error::Error + Send + Sync> {
    e.to_string().into()
}

pub type JobId = Uuid;

/// Which job table a job belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Crawl,
    Embed,
    Extract,
    Ingest,
}

impl JobKind {
    pub fn table_name(self) -> &'static str {
        match self {
            Self::Crawl => "axon_crawl_jobs",
            Self::Embed => "axon_embed_jobs",
            Self::Extract => "axon_extract_jobs",
            Self::Ingest => "axon_ingest_jobs",
        }
    }

    /// Human-readable name used in queue-cap error messages.
    pub fn queue_name(self) -> &'static str {
        match self {
            Self::Crawl => "crawl",
            Self::Embed => "embed",
            Self::Extract => "extract",
            Self::Ingest => "ingest",
        }
    }

    pub fn all() -> &'static [JobKind] {
        &[Self::Crawl, Self::Embed, Self::Extract, Self::Ingest]
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
}

/// Sidecar payload persisted atomically with a job row.
#[derive(Debug, Clone)]
pub enum JobSidecarPayload {
    IngestPreparedSessions { payload_json: String },
}

impl JobSidecarPayload {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::IngestPreparedSessions { .. } => JobKind::Ingest,
        }
    }
}

impl JobPayload {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::Crawl { .. } => JobKind::Crawl,
            Self::Embed { .. } => JobKind::Embed,
            Self::Extract { .. } => JobKind::Extract,
            Self::Ingest { .. } => JobKind::Ingest,
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
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<DateTime<Utc>>,
    pub last_reclaimed_reason: Option<String>,
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

/// Low-level job persistence interface implemented by [`SqliteJobBackend`].
///
/// **Note:** The canonical abstraction consumed by all callers (CLI, MCP, web) is
/// [`ServiceJobRuntime`](crate::services::runtime::ServiceJobRuntime), which
/// returns the richer [`ServiceJob`](crate::services::types::ServiceJob) type.
/// In practice, only `enqueue`, `wait_for_job`, and `job_errors` are delegated through
/// this trait by the service runtime layer; the remaining methods (`list_jobs`,
/// `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs`) are bypassed —
/// `SqliteServiceRuntime` calls `job_query::*` directly, to avoid lossy type mapping
/// from `JobStatusRow`/`JobSummary` to `ServiceJob`.
#[async_trait]
pub trait JobBackend: Send + Sync {
    /// Submit a new job. Returns the assigned JobId.
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId>;

    /// Submit a job and sidecar payload in one SQLite write transaction.
    async fn enqueue_with_sidecar(
        &self,
        payload: JobPayload,
        sidecar: JobSidecarPayload,
    ) -> BackendResult<JobId> {
        let _ = payload;
        let _ = sidecar;
        Err("sidecar enqueue is not supported by this backend".into())
    }

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
    /// Returns the final status string. Used in the SQLite runtime to keep the process
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
            if tokio::time::Instant::now() >= deadline {
                return Err(
                    format!("job {id} timed out after {timeout_secs}s in state running").into(),
                );
            }
            tokio::time::sleep(WAIT_POLL_INTERVAL).await;
        }
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
