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

use axon_core::config::Config;
use axon_jobs::SqliteJobBackend;
use axon_jobs::workers::JobRunnerRegistry;

pub mod drain_lock;
pub mod job_runners;
pub mod sqlite;
pub mod traits;

pub use drain_lock::{DRAIN_LOCK_SUFFIX, WorkerDrainLock, drain_lock_path};
pub use sqlite::SqliteServiceRuntime;
pub use traits::{JobPagination, RuntimeResult, ServiceJobRuntime, WorkerMode};

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
///
/// When workers are spawned, this also builds and hands the unified worker a
/// [`JobRunnerRegistry`] (see [`job_runners`]) so job kinds whose real domain
/// logic lives in `axon-services` (memory compaction, provider probes, …) can
/// execute through the unified worker's claim/dispatch loop instead of always
/// falling back to `job_runner.unsupported_stage`.
pub async fn resolve_runtime_with_workers(
    cfg: Arc<Config>,
    spawn_workers: bool,
) -> Result<Arc<dyn ServiceJobRuntime>, Box<dyn Error + Send + Sync>> {
    if spawn_workers {
        axon_core::health::assert_workers_allowed_by_cutover(&cfg)
            .await
            .map_err(|error| -> Box<dyn Error + Send + Sync> { error.into() })?;
    }
    let backend = if spawn_workers {
        let registry: Arc<JobRunnerRegistry> =
            Arc::new(job_runners::build_registry(&cfg).map_err(|error| {
                format!(
                    "failed to build unified job runner registry; refusing to start workers: {}",
                    error.message
                )
            })?);
        SqliteJobBackend::new_with_workers_and_registry(Arc::clone(&cfg), Some(registry)).await
    } else {
        SqliteJobBackend::new(Arc::clone(&cfg)).await
    }
    .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
    Ok(Arc::new(SqliteServiceRuntime::new_for_backend(
        cfg, backend,
    )))
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
