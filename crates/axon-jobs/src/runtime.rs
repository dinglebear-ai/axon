use std::sync::Arc;

use axon_core::config::Config;
use sqlx::SqlitePool;

use crate::store::{checkpoint_and_close, open_sqlite_pool, open_sqlite_pool_or_recover};
use crate::workers;

/// SQLite job runtime: persistence plus optional in-process unified workers.
///
/// By default, `new()` creates an enqueue-only runtime. Use
/// `new_with_workers()` when the process should also process jobs (for example
/// `axon serve`, MCP, or CLI commands that block on `--wait true`).
pub struct SqliteJobBackend {
    pool: Arc<SqlitePool>,
    workers: Option<workers::WorkerHandles>,
    cfg: Arc<Config>,
}

impl SqliteJobBackend {
    async fn init(pool: Arc<SqlitePool>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Err(error) = crate::store::reclaim_stale_watch_leases(&pool).await {
            if is_lock_busy(&error) {
                tracing::warn!(
                    error = %error,
                    "startup watch-lease reclaim skipped: database is busy"
                );
            } else {
                tracing::error!(error = %error, "startup watch-lease reclaim failed");
            }
        }
        Ok(())
    }

    /// Create an enqueue-only SQLite job runtime (no in-process workers).
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        tracing::info!(
            sqlite_path = %cfg.sqlite_path.display(),
            workers = false,
            "jobs: opening SQLite runtime"
        );
        let pool = Arc::new(open_sqlite_pool_or_recover(&path).await?);
        Self::init(Arc::clone(&pool)).await?;
        Ok(Self {
            pool,
            workers: None,
            cfg,
        })
    }

    /// Create a SQLite job runtime with in-process unified workers.
    pub async fn new_with_workers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new_with_workers_and_registry(cfg, None).await
    }

    /// Like [`Self::new_with_workers`], but accepts a runner registry provided
    /// by `axon-services` so `axon-jobs` can execute domain-specific work
    /// without depending on the services crate.
    pub async fn new_with_workers_and_registry(
        cfg: Arc<Config>,
        job_runner_registry: Option<Arc<workers::JobRunnerRegistry>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = cfg.sqlite_path.to_string_lossy().to_string();
        tracing::info!(
            sqlite_path = %cfg.sqlite_path.display(),
            workers = true,
            "jobs: opening SQLite runtime"
        );
        let pool = Arc::new(open_sqlite_pool_or_recover(&path).await?);
        Self::init(Arc::clone(&pool)).await?;

        let worker_handles =
            workers::spawn_workers(Arc::clone(&pool), Arc::clone(&cfg), job_runner_registry);

        Ok(Self {
            pool,
            workers: Some(worker_handles),
            cfg,
        })
    }

    /// Create a SQLite job runtime with an explicit path (used in tests).
    pub async fn new_with_path(
        path: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = Arc::new(open_sqlite_pool(path).await?);
        Self::init(Arc::clone(&pool)).await?;
        Ok(Self {
            pool,
            workers: None,
            cfg: Arc::new(Config::default_minimal()),
        })
    }

    pub fn pool(&self) -> &Arc<SqlitePool> {
        &self.pool
    }

    pub fn cfg(&self) -> &Config {
        &self.cfg
    }

    /// Graceful shutdown: stop workers, checkpoint the WAL, then close the pool.
    pub async fn shutdown(mut self) {
        drop(self.workers.take());
        checkpoint_and_close(&self.pool).await;
    }

    /// Wake the unified durable-job worker if workers are running. Returns
    /// false when this runtime is enqueue-only.
    pub fn notify_unified(&self) -> bool {
        match &self.workers {
            Some(workers) => {
                workers.notify_unified();
                true
            }
            None => false,
        }
    }
}

fn is_lock_busy(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db) => {
            let message = db.message().to_ascii_lowercase();
            message.contains("database is locked") || message.contains("database is busy")
        }
        _ => false,
    }
}
