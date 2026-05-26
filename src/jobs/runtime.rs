use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::core::config::Config;
use crate::jobs::backend::{
    BackendResult, JobBackend, JobId, JobKind, JobPayload, JobSidecarPayload, JobStatusRow,
    JobSummary,
};
use crate::jobs::ops::is_lock_busy;
use crate::jobs::{ops, query, store, workers};

use crate::jobs::cancel::CancelStore;
use crate::jobs::store::{checkpoint_and_close, open_sqlite_pool, open_sqlite_pool_or_recover};

/// SQLite job backend: persistence plus optional in-process tokio workers.
///
/// By default, `new()` creates an enqueue-only backend (no workers). Use
/// `new_with_workers()` when the process should also process jobs (e.g. `axon serve`
/// or CLI with `--wait true`).
pub struct SqliteJobBackend {
    pool: Arc<SqlitePool>,
    cancel_store: Arc<CancelStore>,
    workers: Option<workers::WorkerHandles>,
    /// Config carried through to `enqueue_job` so pending-queue caps come from
    /// the same priority chain as everywhere else (CLI > env > TOML > default).
    cfg: Arc<Config>,
}

impl SqliteJobBackend {
    /// Shared init: open pool, reclaim stale jobs, create cancel store.
    async fn init(
        pool: Arc<SqlitePool>,
        cfg: &Config,
    ) -> Result<Arc<CancelStore>, Box<dyn std::error::Error + Send + Sync>> {
        let stale_threshold_ms =
            (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
        if let Err(e) = store::reclaim_stale_running_jobs(&pool, stale_threshold_ms).await {
            if is_lock_busy(&e) {
                tracing::warn!(error = %e, "startup reclaim skipped — DB busy; periodic watchdog will retry");
            } else {
                tracing::error!(error = %e, "startup reclaim failed");
            }
        }
        if let Err(e) = store::reclaim_stale_watch_leases(&pool).await {
            if is_lock_busy(&e) {
                tracing::warn!(error = %e, "startup watch-lease reclaim skipped — DB busy");
            } else {
                tracing::error!(error = %e, "startup watch-lease reclaim failed");
            }
        }
        Ok(Arc::new(CancelStore::new()))
    }

    /// Create an enqueue-only SQLite job backend (no in-process workers).
    ///
    /// Jobs are persisted to SQLite but not processed. Use this for CLI
    /// fire-and-forget commands where `axon serve` handles processing.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        tracing::info!(
            sqlite_path = %cfg.sqlite_path.display(),
            workers = false,
            "jobs: opening SQLite backend"
        );
        let pool = Arc::new(open_sqlite_pool_or_recover(&path).await?);
        let cancel_store = Self::init(Arc::clone(&pool), &cfg).await?;

        Ok(Self {
            pool,
            cancel_store,
            workers: None,
            cfg,
        })
    }

    /// Create a SQLite job backend with in-process workers that poll and execute jobs.
    ///
    /// Use this for long-lived processes (`axon serve`, MCP server, web server)
    /// or CLI commands that block until completion (`--wait true`).
    pub async fn new_with_workers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        tracing::info!(
            sqlite_path = %cfg.sqlite_path.display(),
            workers = true,
            "jobs: opening SQLite backend"
        );
        let pool = Arc::new(open_sqlite_pool_or_recover(&path).await?);
        let cancel_store = Self::init(Arc::clone(&pool), &cfg).await?;

        let worker_handles = workers::spawn_workers(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
        );

        Ok(Self {
            pool,
            cancel_store,
            workers: Some(worker_handles),
            cfg,
        })
    }

    /// Create a SQLite job backend with an explicit path (used in tests).
    pub async fn new_with_path(
        path: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = Arc::new(open_sqlite_pool(path).await?);
        let default_cfg = Arc::new(Config::default_minimal());
        let cancel_store = Self::init(Arc::clone(&pool), &default_cfg).await?;

        Ok(Self {
            pool,
            cancel_store,
            workers: None,
            cfg: default_cfg,
        })
    }

    /// Expose the shared SQLite pool for callers that need direct access (e.g. service layer).
    pub fn pool(&self) -> &Arc<SqlitePool> {
        &self.pool
    }

    /// Expose the cancel store so the service layer can fire CancellationTokens on cancel.
    pub fn cancel_store(&self) -> &Arc<CancelStore> {
        &self.cancel_store
    }

    /// Graceful shutdown: stop workers, checkpoint the WAL, then close the pool.
    ///
    /// Call this on SIGTERM before the process exits. Without an explicit
    /// checkpoint, a SIGKILL arriving mid-WAL-write can leave the database in
    /// a corrupted state (`database disk image is malformed`).
    pub async fn shutdown(mut self) {
        // Drop workers first so all in-flight jobs finish or are cancelled.
        drop(self.workers.take());
        // Flush WAL → main database file before releasing the pool.
        checkpoint_and_close(&self.pool).await;
    }

    /// Wake the worker for `kind` if workers are running. Returns false if this backend
    /// is enqueue-only (no workers spawned).
    pub fn notify_worker(&self, kind: JobKind) -> bool {
        match &self.workers {
            Some(w) => {
                w.notify(kind);
                true
            }
            None => false,
        }
    }
}

#[async_trait]
impl JobBackend for SqliteJobBackend {
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId> {
        let kind = payload.kind();
        let id = ops::enqueue_job(&self.pool, &payload, &self.cfg).await?;

        if let Some(ref workers) = self.workers {
            workers.notify(kind);
        }

        Ok(id)
    }

    async fn enqueue_with_sidecar(
        &self,
        payload: JobPayload,
        sidecar: JobSidecarPayload,
    ) -> BackendResult<JobId> {
        let kind = payload.kind();
        let id = ops::enqueue_job_with_sidecar(&self.pool, &payload, &sidecar, &self.cfg).await?;

        if let Some(ref workers) = self.workers {
            workers.notify(kind);
        }

        Ok(id)
    }

    async fn job_status(&self, id: JobId, kind: JobKind) -> BackendResult<Option<JobStatusRow>> {
        Ok(query::job_status_row(&self.pool, kind, id).await?)
    }

    async fn cancel_job(&self, id: JobId, kind: JobKind) -> BackendResult<bool> {
        Ok(self.cancel_store.cancel(id, &self.pool, kind).await?)
    }

    async fn list_jobs(&self, kind: JobKind) -> BackendResult<Vec<JobSummary>> {
        Ok(query::list_jobs(&self.pool, kind).await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        Ok(query::cleanup_jobs(&self.pool, kind).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        Ok(query::clear_jobs(&self.pool, kind).await?)
    }

    async fn job_errors(&self, id: JobId, kind: JobKind) -> BackendResult<Option<String>> {
        Ok(query::job_errors(&self.pool, kind, id).await?)
    }

    /// Override the trait default to use `cfg.job_wait_timeout_secs` (resolved from
    /// CLI > env > TOML > default) instead of reading AXON_JOB_WAIT_TIMEOUT_SECS directly.
    async fn wait_for_job(&self, id: JobId, kind: JobKind) -> BackendResult<String> {
        let timeout_secs = self.cfg.job_wait_timeout_secs;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

        loop {
            match self.job_status(id, kind).await? {
                Some(row) => match row.status {
                    crate::jobs::status::JobStatus::Completed
                    | crate::jobs::status::JobStatus::Failed
                    | crate::jobs::status::JobStatus::Canceled => {
                        return Ok(row.status.as_str().to_string());
                    }
                    _ => {}
                },
                None => {
                    return Err(format!("job {id} not found in {}", kind.table_name()).into());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("job {id} did not complete within {timeout_secs}s").into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_integration_tests.rs"]
mod integration_tests;
