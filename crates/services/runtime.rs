//! Job abstraction for lite mode.
//!
//! This module defines [`ServiceJobRuntime`], the **canonical** backend-agnostic
//! job operations trait consumed by all callers: CLI handlers, MCP handlers, and
//! web routes via [`ServiceContext.jobs`](super::context::ServiceContext).
//!
//! Only `LiteServiceRuntime` (SQLite + in-process workers) is supported. The
//! Postgres + RabbitMQ full backend has been removed.

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload};
use crate::crates::jobs::lite::LiteBackend;
use crate::crates::jobs::lite::query as lite_query;
use crate::crates::jobs::lite::store::reclaim_stale_running_jobs_for_table;
use crate::crates::services::types::ServiceJob;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Started,
    InProcess,
    Unsupported(&'static str),
}

// NOTE: #[async_trait] is required here because this trait is used as
// `dyn ServiceJobRuntime` (object safety). Native async fn in traits (Rust 1.75+)
// uses RPITIT which makes the trait non-object-safe. Once all callers are
// converted to generics, this can be removed.
#[async_trait]
pub trait ServiceJobRuntime: Send + Sync {
    fn mode_name(&self) -> &'static str;

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid>;
    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String>;
    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>>;
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool>;

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>>;
    /// List ingest jobs, optionally filtered by source type.
    ///
    /// **Default implementation:** fetches up to `limit` rows then post-filters
    /// in Rust. This is semantically incorrect when `source_filter` is set and
    /// matching rows number fewer than `limit` — the caller will receive fewer
    /// rows than expected even if more matching rows exist.
    ///
    /// **Override this** in any concrete impl to push the filter into the database.
    /// If a future impl forgets to override, results will be silently wrong for
    /// filtered queries on large tables.
    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        let jobs = self.list_jobs(JobKind::Ingest, limit, offset).await?;
        if let Some(filter) = source_filter {
            Ok(jobs
                .into_iter()
                .filter(|job| job.source_type.as_deref() == Some(filter))
                .collect())
        } else {
            Ok(jobs)
        }
    }
    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>>;
    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>>;

    /// Count all jobs of a given kind using the shared pool.
    ///
    /// Uses the backend's shared SQLite pool directly — avoids calling
    /// `open_sqlite_pool()` (which re-runs migrations on every call) and avoids
    /// bypassing `notify()` on enqueue.
    async fn count_jobs(&self, kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>>;
}

pub async fn resolve_runtime(
    cfg: Arc<Config>,
) -> Result<Arc<dyn ServiceJobRuntime>, Box<dyn Error + Send + Sync>> {
    resolve_runtime_with_workers(cfg, false).await
}

/// Resolve the job runtime, optionally spawning in-process workers.
///
/// `spawn_workers = true` should be used by long-lived processes (`axon serve`,
/// MCP server, web server) or CLI commands that block until completion (`--wait`).
/// `spawn_workers = false` (default via `resolve_runtime`) creates an enqueue-only
/// backend — jobs are persisted but not processed in this process.
pub async fn resolve_runtime_with_workers(
    cfg: Arc<Config>,
    spawn_workers: bool,
) -> Result<Arc<dyn ServiceJobRuntime>, Box<dyn Error + Send + Sync>> {
    let backend = if spawn_workers {
        LiteBackend::new_with_workers(Arc::clone(&cfg)).await
    } else {
        LiteBackend::new(Arc::clone(&cfg)).await
    }
    .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    Ok(Arc::new(LiteServiceRuntime {
        _cfg: cfg,
        backend: Arc::new(backend),
    }))
}

pub struct LiteServiceRuntime {
    _cfg: Arc<Config>,
    backend: Arc<LiteBackend>,
}

#[async_trait]
impl ServiceJobRuntime for LiteServiceRuntime {
    fn mode_name(&self) -> &'static str {
        "lite"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.backend.enqueue(payload).await
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
        Ok(lite_query::list_service_jobs(self.backend.pool(), kind, limit, offset).await?)
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(
            lite_query::list_ingest_service_jobs(self.backend.pool(), source_filter, limit, offset)
                .await?,
        )
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::service_job(self.backend.pool(), kind, id).await?)
    }

    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(self
            .backend
            .cancel_store()
            .cancel(id, self.backend.pool(), kind.table_name())
            .await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::cleanup_jobs(self.backend.pool(), kind.table_name()).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::clear_jobs(self.backend.pool(), kind.table_name()).await?)
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(reclaim_stale_running_jobs_for_table(
            self.backend.pool(),
            kind.table_name(),
            stale_threshold_ms,
        )
        .await?)
    }

    async fn run_worker(&self, _kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        Ok(WorkerMode::InProcess)
    }

    async fn count_jobs(&self, kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::count_jobs(self.backend.pool(), kind.table_name()).await?)
    }
}
