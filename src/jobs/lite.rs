pub mod cancel;
pub mod config_snapshot;
pub mod ops;
pub mod query;
pub mod store;
pub mod workers;

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::core::config::Config;
use crate::jobs::backend::{
    BackendResult, JobBackend, JobId, JobKind, JobPayload, JobStatusRow, JobSummary,
};

use self::cancel::CancelStore;
use self::store::open_sqlite_pool;

/// Lite-mode job backend: SQLite persistence + optional in-process tokio workers.
///
/// By default, `new()` creates an enqueue-only backend (no workers). Use
/// `new_with_workers()` when the process should also process jobs (e.g. `axon serve`
/// or CLI with `--wait true`).
pub struct LiteBackend {
    pool: Arc<SqlitePool>,
    cancel_store: Arc<CancelStore>,
    workers: Option<workers::WorkerHandles>,
    /// Config carried through to `enqueue_job` so pending-queue caps come from
    /// the same priority chain as everywhere else (CLI > env > TOML > default).
    cfg: Arc<Config>,
}

impl LiteBackend {
    /// Shared init: open pool, reclaim stale jobs, create cancel store.
    async fn init(
        pool: Arc<SqlitePool>,
        cfg: &Config,
    ) -> Result<Arc<CancelStore>, Box<dyn std::error::Error + Send + Sync>> {
        let stale_threshold_ms =
            (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
        store::reclaim_stale_running_jobs(&pool, stale_threshold_ms).await?;
        store::reclaim_stale_watch_leases(&pool).await?;
        Ok(Arc::new(CancelStore::new()))
    }

    /// Create an enqueue-only LiteBackend (no in-process workers).
    ///
    /// Jobs are persisted to SQLite but not processed. Use this for CLI
    /// fire-and-forget commands where `axon serve` handles processing.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        tracing::info!(
            sqlite_path = %cfg.sqlite_path.display(),
            workers = false,
            "lite: opening SQLite job backend"
        );
        let pool = Arc::new(open_sqlite_pool(&path).await?);
        let cancel_store = Self::init(Arc::clone(&pool), &cfg).await?;

        Ok(Self {
            pool,
            cancel_store,
            workers: None,
            cfg,
        })
    }

    /// Create a LiteBackend with in-process workers that poll and execute jobs.
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
            "lite: opening SQLite job backend"
        );
        let pool = Arc::new(open_sqlite_pool(&path).await?);
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

    /// Create a LiteBackend with an explicit path (used in tests).
    pub async fn new_with_path(
        path: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = Arc::new(open_sqlite_pool(path).await?);
        let default_cfg = Arc::new(Config::default_lite());
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
impl JobBackend for LiteBackend {
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId> {
        let kind = payload.kind();
        let id = ops::enqueue_job(&self.pool, &payload, &self.cfg).await?;

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
}

#[cfg(test)]
mod tests {
    use super::store::open_sqlite_pool;

    #[tokio::test]
    async fn sqlite_pool_opens_and_tables_exist() {
        let pool = open_sqlite_pool(":memory:")
            .await
            .expect("pool should open");

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'axon_%_jobs'",
        )
        .fetch_one(&pool)
        .await
        .expect("sqlite_master query should work");
        assert_eq!(row.0, 6, "expected 6 job tables");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::jobs::backend::{JobBackend, JobKind, JobPayload};

    #[tokio::test]
    async fn lite_backend_enqueue_and_list() {
        let path = format!("/tmp/axon-test-{}.db", uuid::Uuid::new_v4());
        let backend = LiteBackend::new_with_path(&path)
            .await
            .expect("LiteBackend::new_with_path should succeed");

        let id = backend
            .enqueue(JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            })
            .await
            .expect("enqueue");

        let jobs = backend.list_jobs(JobKind::Crawl).await.expect("list");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn lite_backend_cancel_job() {
        let path = format!("/tmp/axon-test-{}.db", uuid::Uuid::new_v4());
        let backend = LiteBackend::new_with_path(&path).await.unwrap();

        let id = backend
            .enqueue(JobPayload::Embed {
                input: "test".into(),
                config_json: "{}".into(),
            })
            .await
            .unwrap();

        let canceled = backend.cancel_job(id, JobKind::Embed).await.unwrap();
        assert!(canceled);

        let status = backend
            .job_status(id, JobKind::Embed)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.status, crate::jobs::status::JobStatus::Canceled);

        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    #[ignore] // Only runs with 'cargo test -- --ignored' (needs live TEI/Qdrant — not required here)
    async fn lite_backend_full_job_lifecycle() {
        let path = format!("/tmp/axon-e2e-{}.db", uuid::Uuid::new_v4());
        let backend = LiteBackend::new_with_path(&path).await.unwrap();

        let id = backend
            .enqueue(JobPayload::Embed {
                input: "hello world test content for lite mode".into(),
                config_json: "{}".into(),
            })
            .await
            .unwrap();

        // Job should be pending immediately after enqueue
        let status = backend
            .job_status(id, JobKind::Embed)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.status, crate::jobs::status::JobStatus::Pending);

        backend.clear_jobs(JobKind::Embed).await.unwrap();
        std::fs::remove_file(&path).ok();
    }
}
