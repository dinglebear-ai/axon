use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use axon_api::source::{JobKind, LifecycleStatus};
use axon_core::config::Config;
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;
use axon_jobs::status::JobStatus;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_observe::sink::SqliteObservabilitySink;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::runtime::{JobPagination, RuntimeResult, ServiceJobRuntime, WorkerMode};
use crate::types::ServiceJob;

mod service_job_view;

pub struct SqliteServiceRuntime {
    pub(crate) cfg: Arc<Config>,
    pub(crate) backend: Arc<SqliteJobBackend>,
    worker_queue_idle_observed: AtomicBool,
}

impl SqliteServiceRuntime {
    pub(crate) fn new_for_backend(cfg: Arc<Config>, backend: SqliteJobBackend) -> Self {
        Self {
            cfg,
            backend: Arc::new(backend),
            worker_queue_idle_observed: AtomicBool::new(false),
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

    fn worker_in_flight_jobs(&self) -> usize {
        self.backend.worker_in_flight_jobs()
    }

    async fn has_active_worker_jobs(&self, kinds: &[JobKind]) -> RuntimeResult<bool> {
        if kinds.is_empty() {
            return Ok(false);
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT kind FROM jobs \
             WHERE status IN ('queued', 'pending', 'waiting', 'blocked', 'running', 'canceling') \
               AND kind IN (",
        );
        {
            let mut kinds_sql = query.separated(", ");
            for kind in kinds {
                let name = serde_json::to_value(kind)
                    .map_err(Box::<dyn Error + Send + Sync>::from)?
                    .as_str()
                    .ok_or_else(|| {
                        Box::<dyn Error + Send + Sync>::from("job kind did not serialize as a name")
                    })?
                    .to_string();
                kinds_sql.push_bind(name);
            }
        }
        query.push(") LIMIT 1");
        let active_kind = query
            .build_query_scalar::<String>()
            .fetch_optional(self.backend.pool().as_ref())
            .await
            .map_err(Box::<dyn Error + Send + Sync>::from)?;
        if active_kind.is_some() {
            self.worker_queue_idle_observed
                .store(false, Ordering::Release);
            return Ok(true);
        }

        // Emit one diagnostic at the start of each idle window. The database
        // filename comes from the live SQLite connection rather than Config,
        // and the grouped rows expose any unexpected status/kind spelling.
        // This is intentionally transition-triggered so a 30-second idle
        // window does not produce 30 identical log lines.
        if !self.worker_queue_idle_observed.swap(true, Ordering::AcqRel) {
            let database_file = sqlx::query_scalar::<_, String>(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
            )
            .fetch_optional(self.backend.pool().as_ref())
            .await
            .ok()
            .flatten();
            let active_rows = sqlx::query_as::<_, (String, String, i64)>(
                "SELECT kind, status, COUNT(*) \
                   FROM jobs \
                  WHERE status IN ('queued', 'pending', 'waiting', 'blocked', 'running', 'canceling') \
                  GROUP BY kind, status \
                  ORDER BY kind, status",
            )
            .fetch_all(self.backend.pool().as_ref())
            .await
            .unwrap_or_default();
            tracing::warn!(
                configured_path = %self.cfg.sqlite_path.display(),
                database_file = ?database_file,
                active_rows = ?active_rows,
                "jobs: worker queue entered idle window"
            );
        }
        Ok(false)
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

    async fn start_worker(&self, kind: JobKind) -> RuntimeResult<WorkerMode> {
        if !self.backend.notify_unified() {
            return Ok(WorkerMode::Unsupported(
                "no standalone worker in this CLI runtime; use `axon serve` or a command with `--wait true`",
            ));
        }
        self.drain_jobs(kind).await
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
