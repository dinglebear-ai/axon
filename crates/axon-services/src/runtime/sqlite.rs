use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::runtime::{JobPagination, ServiceJobRuntime, WorkerMode};
use crate::types::ServiceJob;
use axon_core::config::Config;
use axon_jobs::SqliteJobBackend;
use axon_jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload, JobSidecarPayload};
use axon_jobs::boundary::JobStore;
use axon_jobs::query as job_query;
use axon_jobs::status::JobStatus;
use axon_jobs::store::reclaim_stale_running_jobs_for_table;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_observe::sink::SqliteObservabilitySink;

mod crawl_bridge;
mod embed_bridge;
mod extract_bridge;
mod ingest_bridge;

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

    /// A fresh unified `JobStore` handle over the same pool this runtime's
    /// `job_status`/`list_jobs`/etc. bridge to for `JobKind::Extract`.
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
        Some(Arc::new(SqliteUnifiedJobStore::with_observe_sink(
            self.backend.pool().as_ref().clone(),
            Arc::new(SqliteObservabilitySink::from_migrated_pool(
                self.backend.pool().as_ref().clone(),
            )),
        )))
    }

    fn notify_unified(&self) {
        self.backend.notify_unified();
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
        if kind == JobKind::Extract {
            let store = self.unified_store();
            return extract_bridge::list(&store, pagination.limit, pagination.offset).await;
        }
        if kind == JobKind::Embed {
            let store = self.unified_store();
            return embed_bridge::list(&store, pagination.limit, pagination.offset).await;
        }
        if kind == JobKind::Crawl {
            let store = self.unified_store();
            return crawl_bridge::list(&store, pagination.limit, pagination.offset).await;
        }
        if kind == JobKind::Ingest {
            let store = self.unified_store();
            return ingest_bridge::list(&store, pagination.limit, pagination.offset).await;
        }
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
        if kind == JobKind::Extract {
            return extract_bridge::status(&self.unified_store(), id).await;
        }
        if kind == JobKind::Embed {
            return embed_bridge::status(&self.unified_store(), id).await;
        }
        if kind == JobKind::Crawl {
            return crawl_bridge::status(&self.unified_store(), id).await;
        }
        if kind == JobKind::Ingest {
            return ingest_bridge::status(&self.unified_store(), id).await;
        }
        Ok(job_query::service_job(self.backend.pool(), kind, id).await?)
    }

    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        if kind == JobKind::Extract {
            return extract_bridge::cancel(&self.unified_store(), id).await;
        }
        if kind == JobKind::Embed {
            return embed_bridge::cancel(&self.unified_store(), id).await;
        }
        if kind == JobKind::Crawl {
            return crawl_bridge::cancel(&self.unified_store(), id).await;
        }
        if kind == JobKind::Ingest {
            return ingest_bridge::cancel(&self.unified_store(), id).await;
        }
        Ok(self
            .backend
            .cancel_store()
            .cancel(id, self.backend.pool(), kind)
            .await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        if kind == JobKind::Extract {
            return extract_bridge::cleanup(&self.unified_store()).await;
        }
        if kind == JobKind::Embed {
            return embed_bridge::cleanup(&self.unified_store()).await;
        }
        if kind == JobKind::Crawl {
            return crawl_bridge::cleanup(&self.unified_store()).await;
        }
        if kind == JobKind::Ingest {
            return ingest_bridge::cleanup(&self.unified_store()).await;
        }
        Ok(job_query::cleanup_jobs(self.backend.pool(), kind).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        if kind == JobKind::Extract {
            return extract_bridge::clear(&self.unified_store()).await;
        }
        if kind == JobKind::Embed {
            return embed_bridge::clear(&self.unified_store()).await;
        }
        if kind == JobKind::Crawl {
            return crawl_bridge::clear(&self.unified_store()).await;
        }
        if kind == JobKind::Ingest {
            return ingest_bridge::clear(&self.unified_store()).await;
        }
        Ok(job_query::clear_jobs(self.backend.pool(), kind).await?)
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        if kind == JobKind::Extract {
            return extract_bridge::recover(&self.unified_store(), stale_threshold_ms).await;
        }
        if kind == JobKind::Embed {
            return embed_bridge::recover(&self.unified_store(), stale_threshold_ms).await;
        }
        if kind == JobKind::Crawl {
            return crawl_bridge::recover(&self.unified_store(), stale_threshold_ms).await;
        }
        if kind == JobKind::Ingest {
            return ingest_bridge::recover(&self.unified_store(), stale_threshold_ms).await;
        }
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
        if let Some(unified_kind) = unified_kind_for_bridged(kind) {
            return Ok(unified_job_summaries(&self.unified_store(), unified_kind)
                .await?
                .len() as i64);
        }
        Ok(job_query::count_jobs(self.backend.pool(), kind).await?)
    }

    async fn count_jobs_by_status(
        &self,
        kind: JobKind,
    ) -> Result<std::collections::HashMap<JobStatus, i64>, Box<dyn Error + Send + Sync>> {
        if let Some(unified_kind) = unified_kind_for_bridged(kind) {
            let summaries = unified_job_summaries(&self.unified_store(), unified_kind).await?;
            let mut out: std::collections::HashMap<JobStatus, i64> =
                std::collections::HashMap::new();
            for summary in summaries {
                let key = JobStatus::from_str(bridge_legacy_status(summary.status));
                *out.entry(key).or_insert(0) += 1;
            }
            return Ok(out);
        }
        Ok(job_query::count_jobs_by_status(self.backend.pool(), kind).await?)
    }
}

/// Map a bridged legacy `JobKind` onto its unified `axon_api::source::JobKind`
/// counterpart. Returns `None` for kinds still fully on the legacy backend
/// (there are none today — Extract/Embed/Crawl/Ingest are all bridged — but
/// this keeps `count_jobs`/`count_jobs_by_status` consistent with the other
/// `if kind == JobKind::X { bridge... }` dispatch in this file, and fails
/// closed to the legacy path for any future kind added here without a bridge).
fn unified_kind_for_bridged(kind: JobKind) -> Option<axon_api::source::JobKind> {
    match kind {
        JobKind::Extract => Some(axon_api::source::JobKind::Extract),
        JobKind::Embed => Some(axon_api::source::JobKind::Embed),
        JobKind::Crawl => Some(axon_api::source::JobKind::Crawl),
        JobKind::Ingest => Some(axon_api::source::JobKind::Ingest),
    }
}

/// Legacy 5-value status string for a unified `LifecycleStatus`, matching
/// each bridge module's own `legacy_status()` collapse so `count_jobs_by_status`
/// histograms agree with `list`/`status`'s rendered `ServiceJob.status`.
fn bridge_legacy_status(status: axon_api::source::LifecycleStatus) -> &'static str {
    use axon_api::source::LifecycleStatus;
    match status {
        LifecycleStatus::Queued
        | LifecycleStatus::Pending
        | LifecycleStatus::Waiting
        | LifecycleStatus::Blocked => "pending",
        LifecycleStatus::Running | LifecycleStatus::Canceling => "running",
        LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded => "completed",
        LifecycleStatus::Failed | LifecycleStatus::Expired => "failed",
        LifecycleStatus::Canceled | LifecycleStatus::Skipped => "canceled",
    }
}

/// All `JobSummary` rows for one unified `job_kind`. Job volumes for the
/// bridged kinds are low enough (see `extract_bridge::list`'s own comment)
/// that a single bounded fetch-and-count is an acceptable cost here — same
/// tradeoff already accepted by the per-kind bridge `list` functions.
async fn unified_job_summaries(
    store: &Arc<dyn JobStore>,
    kind: axon_api::source::JobKind,
) -> Result<Vec<axon_api::source::JobSummary>, Box<dyn Error + Send + Sync>> {
    let page = store
        .list(axon_api::source::JobListRequest {
            status: None,
            kind: Some(kind),
            source_id: None,
            watch_id: None,
            limit: Some(1000),
            cursor: None,
        })
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(page.items)
}
