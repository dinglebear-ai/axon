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

    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        if !self.backend.notify_worker(kind) {
            return Err("no in-process workers running — use `axon serve` or `--wait true`".into());
        }
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
        Ok(lite_query::count_jobs(self.backend.pool(), kind.table_name()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::{enqueue_job, mark_completed, mark_failed};
    use crate::crates::jobs::lite::store::open_sqlite_pool;
    use sqlx::SqlitePool;
    use std::time::Duration;
    use uuid::Uuid;

    async fn fresh_pool() -> SqlitePool {
        open_sqlite_pool(":memory:").await.expect("pool")
    }

    /// has_active_jobs is per-kind: a pending row in another table must NOT make
    /// us think the queried kind has active jobs. (bd axon_rust-cr5.14)
    #[tokio::test]
    async fn has_active_jobs_is_isolated_per_kind() {
        let pool = fresh_pool().await;
        // Seed a pending crawl job.
        enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue crawl");

        // Seed a pending embed job.
        enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "doc".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue embed");

        let active_crawl = has_active_for_kind(&pool, JobKind::Crawl).await;
        let active_embed = has_active_for_kind(&pool, JobKind::Embed).await;
        let active_extract = has_active_for_kind(&pool, JobKind::Extract).await;

        assert!(active_crawl, "crawl table has pending row");
        assert!(active_embed, "embed table has pending row");
        assert!(
            !active_extract,
            "extract table is empty — must not be considered active"
        );
    }

    /// Once all jobs for a kind reach a terminal state (completed/failed/canceled),
    /// has_active_jobs returns false even if other kinds still have pending rows.
    #[tokio::test]
    async fn has_active_jobs_false_after_terminal_states() {
        let pool = fresh_pool().await;
        let crawl_id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue crawl");
        // Seed an unrelated pending embed row that should NOT block the crawl drain.
        let _ = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "doc".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue embed");

        // Move crawl to running, then completed.
        super::super::super::jobs::lite::ops::claim_next_pending(&pool, "axon_crawl_jobs")
            .await
            .expect("claim crawl");
        mark_completed(&pool, "axon_crawl_jobs", crawl_id, None)
            .await
            .expect("complete crawl");

        let active_crawl = has_active_for_kind(&pool, JobKind::Crawl).await;
        let active_embed = has_active_for_kind(&pool, JobKind::Embed).await;
        assert!(
            !active_crawl,
            "crawl drain should see no active rows once completed"
        );
        assert!(active_embed, "embed remains pending — unrelated kind");
    }

    /// Bounded-time drain: once the queried kind has no pending/running rows,
    /// the wait loop returns within ~1s even if other kinds still have rows.
    #[tokio::test]
    async fn drain_terminates_quickly_on_terminal_state() {
        let pool = fresh_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");
        // Seed unrelated pending embed row that must not stall the crawl drain.
        let _ = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "x".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue embed");

        // Mark crawl failed (terminal).
        super::super::super::jobs::lite::ops::claim_next_pending(&pool, "axon_crawl_jobs")
            .await
            .expect("claim");
        mark_failed(&pool, "axon_crawl_jobs", id, "test")
            .await
            .expect("fail");

        // Simulate the run_worker drain wait: should return immediately for crawl.
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            let mut iters = 0;
            loop {
                if !has_active_for_kind(&pool, JobKind::Crawl).await {
                    break iters;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                iters += 1;
                if iters > 40 {
                    break iters;
                }
            }
        })
        .await
        .expect("drain wait must not hang past 2s");
        assert!(
            result < 5,
            "drain should exit immediately for terminal-state crawl, got {result} iters"
        );
    }

    /// Mirror of LiteServiceRuntime::has_active_jobs that operates on a raw
    /// pool — lets tests exercise the same predicate without constructing a
    /// full LiteBackend.
    async fn has_active_for_kind(pool: &SqlitePool, kind: JobKind) -> bool {
        let table = kind.table_name();
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE status IN ('pending','running') LIMIT 1)",
            table
        );
        sqlx::query_scalar::<_, bool>(&sql)
            .fetch_one(pool)
            .await
            .unwrap_or(false)
    }

    // Silence "unused" lints when only the helper is built without the tests.
    #[allow(dead_code)]
    fn _force_uuid_use() {
        let _ = Uuid::new_v4();
    }
}
