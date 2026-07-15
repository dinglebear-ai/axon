use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{JobKind, LifecycleStatus};
use axon_core::config::Config;
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;
use axon_jobs::status::JobStatus;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_observe::sink::SqliteObservabilitySink;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::runtime::{JobPagination, RuntimeResult, ServiceJobRuntime, WorkerMode};
use crate::types::ServiceJob;

mod service_job_view;

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

    fn unified_store(&self) -> Arc<dyn JobStore> {
        Arc::new(SqliteUnifiedJobStore::with_observe_sink(
            self.backend.pool().as_ref().clone(),
            Arc::new(SqliteObservabilitySink::from_migrated_pool(
                self.backend.pool().as_ref().clone(),
            )),
        ))
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

    fn unified_job_store(&self) -> Option<Arc<dyn JobStore>> {
        Some(self.unified_store())
    }

    fn notify_unified(&self) {
        self.backend.notify_unified();
    }

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> RuntimeResult<String> {
        let timeout_secs = self.cfg.job_wait_timeout_secs;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        let store = self.unified_store();
        loop {
            let Some(job) = store
                .get(axon_api::source::JobId::new(id))
                .await
                .map_err(|error| Box::<dyn Error + Send + Sync>::from(error.message))?
            else {
                return Err(format!("job {id} not found").into());
            };
            if job.kind != kind {
                return Err(format!("job {id} is {:?}, not {:?}", job.kind, kind).into());
            }
            if matches!(
                job.status,
                LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
                    | LifecycleStatus::Failed
                    | LifecycleStatus::Canceled
                    | LifecycleStatus::Expired
                    | LifecycleStatus::Skipped
            ) {
                return Ok(format!("{:?}", job.status).to_ascii_lowercase());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("job {id} did not complete within {timeout_secs}s").into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    async fn job_errors(&self, id: Uuid, kind: JobKind) -> RuntimeResult<Option<String>> {
        Ok(self
            .job_status(kind, id)
            .await?
            .and_then(|job| job.error_text))
    }

    async fn has_active_jobs(&self, kind: JobKind) -> RuntimeResult<bool> {
        let store = self.unified_store();
        for status in [
            LifecycleStatus::Queued,
            LifecycleStatus::Pending,
            LifecycleStatus::Waiting,
            LifecycleStatus::Blocked,
            LifecycleStatus::Running,
            LifecycleStatus::Canceling,
        ] {
            let page = store
                .list(axon_api::source::JobListRequest {
                    status: Some(status),
                    kind: Some(kind),
                    source_id: None,
                    watch_id: None,
                    limit: Some(1),
                    cursor: None,
                })
                .await
                .map_err(|error| Box::<dyn Error + Send + Sync>::from(error.message))?;
            if !page.items.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> RuntimeResult<Vec<ServiceJob>> {
        let pagination = JobPagination::new(limit, offset)?;
        service_job_view::list(
            &self.unified_store(),
            kind,
            pagination.limit,
            pagination.offset,
        )
        .await
    }

    async fn job_status(&self, kind: JobKind, id: Uuid) -> RuntimeResult<Option<ServiceJob>> {
        service_job_view::status(&self.unified_store(), kind, id).await
    }

    async fn cancel_job(&self, kind: JobKind, id: Uuid) -> RuntimeResult<bool> {
        service_job_view::cancel(
            &self.unified_store(),
            id,
            format!("cancel requested for {:?} job", kind).to_ascii_lowercase(),
        )
        .await
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> RuntimeResult<u64> {
        service_job_view::cleanup(&self.unified_store(), kind).await
    }

    async fn clear_jobs(&self, kind: JobKind) -> RuntimeResult<u64> {
        service_job_view::cleanup(&self.unified_store(), kind).await
    }

    async fn recover_jobs(&self, kind: JobKind, stale_threshold_ms: i64) -> RuntimeResult<u64> {
        service_job_view::recover(&self.unified_store(), kind, stale_threshold_ms).await
    }

    async fn notify_worker(&self, _kind: JobKind) -> RuntimeResult<()> {
        if !self.backend.notify_unified() {
            return Err(
                "no in-process workers running -- use `axon serve` or `--wait true`".into(),
            );
        }
        Ok(())
    }

    async fn drain_jobs(&self, kind: JobKind) -> RuntimeResult<WorkerMode> {
        let pending_at_start = self.count_jobs(kind).await.unwrap_or(0);
        tracing::info!(?kind, pending_at_start, "draining job queue");
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(self.cfg.job_wait_timeout_secs.max(1));
        loop {
            if !self.has_active_jobs(kind).await? {
                break;
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "drain_jobs timed out after {}s while draining {:?} jobs",
                    timeout.as_secs(),
                    kind
                )
                .into());
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let elapsed_secs = started.elapsed().as_secs();
            if elapsed_secs > 0 && elapsed_secs.is_multiple_of(10) {
                tracing::info!(?kind, elapsed_secs, "still draining job queue");
            }
        }
        Ok(WorkerMode::InProcess {
            pending_at_start,
            elapsed_secs: started.elapsed().as_secs(),
        })
    }

    async fn count_jobs(&self, kind: JobKind) -> RuntimeResult<i64> {
        service_job_view::count(&self.unified_store(), kind).await
    }

    async fn count_jobs_by_status(
        &self,
        kind: JobKind,
    ) -> RuntimeResult<std::collections::HashMap<JobStatus, i64>> {
        service_job_view::count_by_status(&self.unified_store(), kind).await
    }
}
