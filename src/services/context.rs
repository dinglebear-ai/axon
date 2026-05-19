use std::sync::Arc;
use std::time::Duration;

use crate::core::config::Config;
use crate::jobs::backend::JobKind;
use crate::services::runtime::{ServiceJobRuntime, resolve_runtime_with_workers};

#[derive(Clone)]
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub jobs: Arc<dyn ServiceJobRuntime>,
}

impl ServiceContext {
    async fn build(
        cfg: Arc<Config>,
        spawn_workers: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let jobs = resolve_runtime_with_workers(Arc::clone(&cfg), spawn_workers).await?;
        if spawn_workers {
            spawn_queue_summary_logger(Arc::clone(&jobs), cfg.queue_summary_secs);
        }
        Ok(Self { cfg, jobs })
    }

    /// Create a ServiceContext without in-process workers (enqueue-only in the SQLite runtime).
    ///
    /// This is the safe default for CLI commands that enqueue and exit.
    /// Use `new_with_workers()` for long-lived processes that should process jobs.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, false).await
    }

    /// Create a ServiceContext with in-process workers (SQLite runtime only).
    ///
    /// Use for `axon serve`, MCP server, web server, or CLI `--wait true`.
    pub async fn new_with_workers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, true).await
    }

    /// Factory for test helpers — inject a mock `ServiceJobRuntime`.
    pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        Self { cfg, jobs }
    }

    pub fn with_jobs_runtime(mut self, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        self.jobs = jobs;
        self
    }
}

/// Periodic queue-depth summary logger for log-based monitoring.
///
/// Spawned only by `new_with_workers()` (so worker-bearing processes — serve,
/// mcp — emit a baseline queue signal). Interval is `AXON_QUEUE_SUMMARY_SECS`
/// (default 30s).
fn spawn_queue_summary_logger(jobs: Arc<dyn ServiceJobRuntime>, secs: u64) {
    if secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(secs));
        // Skip the initial fire so the first log is one period in, not at startup.
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(crawl) = queue_depth(&jobs, JobKind::Crawl).await else {
                continue;
            };
            let Some(extract) = queue_depth(&jobs, JobKind::Extract).await else {
                continue;
            };
            let Some(embed) = queue_depth(&jobs, JobKind::Embed).await else {
                continue;
            };
            let Some(ingest) = queue_depth(&jobs, JobKind::Ingest).await else {
                continue;
            };
            tracing::info!(
                crawl,
                extract,
                embed,
                ingest,
                interval_secs = secs,
                "job queue summary"
            );
        }
    });
}

async fn queue_depth(jobs: &Arc<dyn ServiceJobRuntime>, kind: JobKind) -> Option<i64> {
    match jobs.count_jobs(kind).await {
        Ok(count) => Some(count),
        Err(err) => {
            tracing::warn!(
                ?kind,
                error = %err,
                "failed to read job queue depth"
            );
            None
        }
    }
}
