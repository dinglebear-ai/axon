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

use crate::core::config::Config;
use crate::jobs::SqliteJobBackend;

pub mod sqlite;
pub mod traits;

pub use sqlite::SqliteServiceRuntime;
pub use traits::{JobPagination, ServiceJobRuntime, WorkerMode};

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
    Ok(Arc::new(SqliteServiceRuntime::new_for_backend(
        cfg, backend,
    )))
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
