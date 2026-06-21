use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::core::config::Config;
use crate::jobs::SqliteJobBackend;
use crate::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload, JobSidecarPayload};
use crate::jobs::query as job_query;
use crate::jobs::status::JobStatus;
use crate::jobs::store::reclaim_stale_running_jobs_for_table;
use crate::services::runtime::{JobPagination, ServiceJobRuntime, WorkerMode};
use crate::services::types::ServiceJob;

pub struct SqliteServiceRuntime {
    pub(crate) cfg: Arc<Config>,
    pub(crate) backend: Arc<SqliteJobBackend>,
}

impl SqliteServiceRuntime {
    pub(crate) fn new_for_backend(cfg: Arc<Config>, backend: SqliteJobBackend) -> Self {
        Self {
            cfg,
            backend: Arc::new(backend),
        }
    }
}

#[async_trait]
impl ServiceJobRuntime for SqliteServiceRuntime {
    fn mode_name(&self) -> &'static str {
        "sqlite"
    }

    fn sqlite_pool(&self) -> Option<Arc<SqlitePool>> {
        Some(Arc::clone(self.backend.pool()))
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.backend.enqueue(payload).await
    }

    async fn enqueue_with_sidecar(
        &self,
        payload: JobPayload,
        sidecar: JobSidecarPayload,
    ) -> BackendResult<Uuid> {
        self.backend.enqueue_with_sidecar(payload, sidecar).await
    }

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String> {
        self.backend.wait_for_job(id, kind).await
    }

    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>> {
        self.backend.job_errors(id, kind).await
    }

    /// SQL EXISTS check against the cached pool — avoids fetching all rows.
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool> {
        let table = kind.table_name();
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE status IN ('pending','running') LIMIT 1)",
            table,
        );
        let exists: bool = sqlx::query_scalar(&sql)
            .fetch_one(self.backend.pool().as_ref())
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        Ok(exists)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        let pagination = JobPagination::new(limit, offset)?;
        Ok(job_query::list_service_jobs(
            self.backend.pool(),
            kind,
            pagination.limit,
            pagination.offset,
        )
        .await?)
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        let pagination = JobPagination::new(limit, offset)?;
        Ok(job_query::list_ingest_service_jobs(
            self.backend.pool(),
            source_filter,
            pagination.limit,
            pagination.offset,
        )
        .await?)
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(job_query::service_job(self.backend.pool(), kind, id).await?)
    }

    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(self
            .backend
            .cancel_store()
            .cancel(id, self.backend.pool(), kind)
            .await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(job_query::cleanup_jobs(self.backend.pool(), kind).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(job_query::clear_jobs(self.backend.pool(), kind).await?)
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(reclaim_stale_running_jobs_for_table(
            self.backend.pool(),
            kind,
            stale_threshold_ms,
            self.backend.cfg().max_job_attempts,
        )
        .await?)
    }

    async fn notify_worker(&self, kind: JobKind) -> Result<(), Box<dyn Error + Send + Sync>> {
        if !self.backend.notify_worker(kind) {
            return Err("no in-process workers running — use `axon serve` or `--wait true`".into());
        }
        Ok(())
    }

    async fn drain_jobs(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        let pending_at_start = self.count_jobs(kind).await.unwrap_or(0);
        tracing::info!(
            queue = kind.table_name(),
            pending_at_start,
            "draining job queue"
        );
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(self.cfg.job_wait_timeout_secs.max(1));
        loop {
            if !self.has_active_jobs(kind).await? {
                break;
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "drain_jobs timed out after {}s while draining {} queue",
                    timeout.as_secs(),
                    kind.table_name()
                )
                .into());
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let elapsed_secs = started.elapsed().as_secs();
            if elapsed_secs > 0 && elapsed_secs.is_multiple_of(10) {
                tracing::info!(
                    queue = kind.table_name(),
                    elapsed_secs,
                    "still draining job queue"
                );
            }
        }
        Ok(WorkerMode::InProcess {
            pending_at_start,
            elapsed_secs: started.elapsed().as_secs(),
        })
    }

    async fn count_jobs(&self, kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(job_query::count_jobs(self.backend.pool(), kind).await?)
    }

    async fn count_jobs_by_status(
        &self,
        kind: JobKind,
    ) -> Result<std::collections::HashMap<JobStatus, i64>, Box<dyn Error + Send + Sync>> {
        Ok(job_query::count_jobs_by_status(self.backend.pool(), kind).await?)
    }
}
