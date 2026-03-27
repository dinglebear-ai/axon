pub mod cancel;
pub mod ops;
pub mod query;
pub mod store;
pub mod workers;

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{
    BackendResult, JobBackend, JobId, JobKind, JobPayload, JobStatusRow, JobSummary,
};

use self::cancel::CancelStore;
use self::store::open_sqlite_pool;

/// Lite-mode job backend: SQLite persistence + in-process tokio workers.
pub struct LiteBackend {
    pool: Arc<SqlitePool>,
    cancel_store: Arc<CancelStore>,
    workers: workers::WorkerHandles,
}

impl LiteBackend {
    /// Create a new LiteBackend using the SQLite path from Config.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        let pool = Arc::new(open_sqlite_pool(&path).await?);

        let stale_threshold_ms =
            (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
        store::reclaim_stale_running_jobs(&pool, stale_threshold_ms).await?;

        let cancel_store = Arc::new(CancelStore::new());
        let worker_handles = workers::spawn_workers(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
        );

        Ok(Self {
            pool,
            cancel_store,
            workers: worker_handles,
        })
    }

    /// Create a LiteBackend with an explicit path (used in tests).
    pub async fn new_with_path(
        path: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = Arc::new(open_sqlite_pool(path).await?);

        let stale_threshold_ms = (300 + 60) * 1_000i64;
        store::reclaim_stale_running_jobs(&pool, stale_threshold_ms).await?;

        let cancel_store = Arc::new(CancelStore::new());
        let dummy_cfg = Arc::new(Config::default_lite());
        let worker_handles =
            workers::spawn_workers(Arc::clone(&pool), dummy_cfg, Arc::clone(&cancel_store));

        Ok(Self {
            pool,
            cancel_store,
            workers: worker_handles,
        })
    }

    fn table_for(kind: JobKind) -> &'static str {
        kind.table_name()
    }
}

#[async_trait]
impl JobBackend for LiteBackend {
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId> {
        let kind = payload.kind();
        let id = ops::enqueue_job(&self.pool, &payload).await?;

        match kind {
            JobKind::Crawl => self.workers.crawl.notify_one(),
            JobKind::Embed => self.workers.embed.notify_one(),
            JobKind::Extract => self.workers.extract.notify_one(),
            JobKind::Ingest => self.workers.ingest.notify_one(),
            JobKind::Refresh => self.workers.refresh.notify_one(),
            JobKind::Graph => self.workers.graph.notify_one(),
        }

        Ok(id)
    }

    async fn job_status(&self, id: JobId, kind: JobKind) -> BackendResult<Option<JobStatusRow>> {
        Ok(query::job_status_row(&self.pool, Self::table_for(kind), id).await?)
    }

    async fn cancel_job(&self, id: JobId, kind: JobKind) -> BackendResult<bool> {
        Ok(self
            .cancel_store
            .cancel(id, &self.pool, Self::table_for(kind))
            .await?)
    }

    async fn list_jobs(&self, kind: JobKind) -> BackendResult<Vec<JobSummary>> {
        Ok(query::list_jobs(&self.pool, Self::table_for(kind)).await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        Ok(query::cleanup_jobs(&self.pool, Self::table_for(kind)).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        Ok(query::clear_jobs(&self.pool, Self::table_for(kind)).await?)
    }

    async fn job_errors(&self, id: JobId, kind: JobKind) -> BackendResult<Option<String>> {
        Ok(query::job_errors(&self.pool, Self::table_for(kind), id).await?)
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
    use crate::crates::jobs::backend::{JobBackend, JobKind, JobPayload};

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
        assert_eq!(
            status.status,
            crate::crates::jobs::status::JobStatus::Canceled
        );

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
        assert_eq!(
            status.status,
            crate::crates::jobs::status::JobStatus::Pending
        );

        backend.clear_jobs(JobKind::Embed).await.unwrap();
        std::fs::remove_file(&path).ok();
    }
}
