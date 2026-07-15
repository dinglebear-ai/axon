use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::types::ServiceJob;
use axon_api::source::JobKind;
use axon_jobs::boundary::JobStore;
use axon_jobs::status::JobStatus;

pub type RuntimeResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Started,
    /// In-process worker drained the queue. `pending_at_start` records the
    /// number of pending+running jobs observed at the start of the drain;
    /// `elapsed_secs` is wall-clock seconds spent waiting.
    InProcess {
        pending_at_start: i64,
        elapsed_secs: u64,
    },
    Unsupported(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobPagination {
    pub limit: i64,
    pub offset: i64,
}

impl JobPagination {
    pub fn new(limit: i64, offset: i64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        if limit < 0 {
            return Err(format!("job pagination limit must be non-negative, got {limit}").into());
        }
        if offset < 0 {
            return Err(format!("job pagination offset must be non-negative, got {offset}").into());
        }
        Ok(Self { limit, offset })
    }
}

// NOTE: #[async_trait] is required here because this trait is used as
// `dyn ServiceJobRuntime` (object safety). Native async fn in traits (Rust 1.75+)
// uses RPITIT which makes the trait non-object-safe. Once all callers are
// converted to generics, this can be removed.
#[async_trait]
pub trait ServiceJobRuntime: Send + Sync {
    fn mode_name(&self) -> &'static str;

    fn sqlite_pool(&self) -> Option<Arc<SqlitePool>> {
        None
    }

    fn unified_job_store(&self) -> Option<Arc<dyn JobStore>> {
        None
    }

    /// Wake the unified durable-job worker (if this runtime has in-process
    /// workers) so a freshly enqueued job is claimed on its next wakeup
    /// instead of waiting for the worker's poll interval. A no-op for
    /// enqueue-only runtimes (no workers spawned) — mirrors `notify_worker`'s
    /// "best-effort, false if unsupported" contract but is infallible since
    /// callers treat a missed wakeup as a (slower, still-correct) poll fallback.
    fn notify_unified(&self) {}

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> RuntimeResult<String>;
    async fn job_errors(&self, id: Uuid, kind: JobKind) -> RuntimeResult<Option<String>>;
    async fn has_active_jobs(&self, kind: JobKind) -> RuntimeResult<bool>;
    async fn notify_worker(&self, kind: JobKind) -> RuntimeResult<()> {
        let _ = kind;
        Err("worker notifications are not supported by this runtime".into())
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> RuntimeResult<Vec<ServiceJob>>;
    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> RuntimeResult<Vec<ServiceJob>> {
        if source_filter.is_some() {
            return Err(
                "filtered ingest pagination is not supported by this runtime implementation".into(),
            );
        }
        self.list_jobs(JobKind::Source, limit, offset).await
    }
    async fn job_status(&self, kind: JobKind, id: Uuid) -> RuntimeResult<Option<ServiceJob>>;
    async fn cancel_job(&self, kind: JobKind, id: Uuid) -> RuntimeResult<bool>;
    async fn cleanup_jobs(&self, kind: JobKind) -> RuntimeResult<u64>;
    async fn clear_jobs(&self, kind: JobKind) -> RuntimeResult<u64>;
    async fn recover_jobs(&self, kind: JobKind, stale_threshold_ms: i64) -> RuntimeResult<u64>;
    async fn drain_jobs(&self, kind: JobKind) -> RuntimeResult<WorkerMode> {
        let _ = kind;
        Ok(WorkerMode::Unsupported(
            "queue draining is not supported by this runtime",
        ))
    }

    async fn start_worker(&self, kind: JobKind) -> RuntimeResult<WorkerMode> {
        self.notify_worker(kind).await?;
        self.drain_jobs(kind).await
    }

    async fn count_jobs(&self, kind: JobKind) -> RuntimeResult<i64>;
    async fn count_jobs_by_status(&self, kind: JobKind) -> RuntimeResult<HashMap<JobStatus, i64>>;
}
