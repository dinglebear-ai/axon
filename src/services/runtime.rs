//! Job abstraction for the SQLite in-process runtime.
//!
//! This module defines [`ServiceJobRuntime`], the **canonical** backend-agnostic
//! job operations trait consumed by all callers: CLI handlers, MCP handlers, and
//! web routes via [`ServiceContext.jobs`](super::context::ServiceContext).
//!
//! Only `SqliteServiceRuntime` (SQLite + in-process workers) is supported. The
//! Postgres + RabbitMQ full backend has been removed.

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::core::config::Config;
use crate::jobs::SqliteJobBackend;
use crate::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload};
use crate::jobs::query as job_query;
use crate::jobs::store::reclaim_stale_running_jobs_for_table;
use crate::services::types::ServiceJob;

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

// NOTE: #[async_trait] is required here because this trait is used as
// `dyn ServiceJobRuntime` (object safety). Native async fn in traits (Rust 1.75+)
// uses RPITIT which makes the trait non-object-safe. Once all callers are
// converted to generics, this can be removed.
#[async_trait]
pub trait ServiceJobRuntime: Send + Sync {
    fn mode_name(&self) -> &'static str;

    /// Return the runtime's shared SQLite pool when this runtime is backed by
    /// SQLite. Long-lived surfaces can use this to avoid opening a separate
    /// pool and re-running migrations for adjacent scheduler/watch operations.
    fn sqlite_pool(&self) -> Option<Arc<SqlitePool>> {
        None
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid>;
    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String>;
    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>>;
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool>;
    async fn notify_worker(&self, kind: JobKind) -> Result<(), Box<dyn Error + Send + Sync>> {
        let _ = kind;
        Err("worker notifications are not supported by this runtime".into())
    }

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
    async fn drain_jobs(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        let _ = kind;
        Ok(WorkerMode::Unsupported(
            "queue draining is not supported by this runtime",
        ))
    }

    async fn start_worker(
        &self,
        kind: JobKind,
    ) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        self.notify_worker(kind).await?;
        self.drain_jobs(kind).await
    }

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
        SqliteJobBackend::new_with_workers(Arc::clone(&cfg)).await
    } else {
        SqliteJobBackend::new(Arc::clone(&cfg)).await
    }
    .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    Ok(Arc::new(SqliteServiceRuntime {
        _cfg: cfg,
        backend: Arc::new(backend),
    }))
}

pub struct SqliteServiceRuntime {
    _cfg: Arc<Config>,
    backend: Arc<SqliteJobBackend>,
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
        Ok(job_query::list_service_jobs(self.backend.pool(), kind, limit, offset).await?)
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(
            job_query::list_ingest_service_jobs(self.backend.pool(), source_filter, limit, offset)
                .await?,
        )
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
        Ok(
            reclaim_stale_running_jobs_for_table(self.backend.pool(), kind, stale_threshold_ms)
                .await?,
        )
    }

    async fn notify_worker(&self, kind: JobKind) -> Result<(), Box<dyn Error + Send + Sync>> {
        if !self.backend.notify_worker(kind) {
            return Err("no in-process workers running — use `axon serve` or `--wait true`".into());
        }
        Ok(())
    }

    async fn drain_jobs(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        let pending_at_start = self.count_jobs(kind).await.unwrap_or(0);
        eprintln!(
            "draining {} queue ({pending_at_start} pending)...",
            kind.table_name()
        );
        let started = std::time::Instant::now();
        let mut secs = 0u64;
        loop {
            if !self.has_active_jobs(kind).await? {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            secs += 1;
            if secs.is_multiple_of(10) {
                eprintln!("still draining ({secs}s elapsed)...");
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
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
