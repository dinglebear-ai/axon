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
            spawn_queue_summary_logger(Arc::clone(&jobs));
        }
        Ok(Self { cfg, jobs })
    }

    /// Create a ServiceContext without in-process workers (enqueue-only in lite mode).
    ///
    /// This is the safe default for CLI commands that enqueue and exit.
    /// Use `new_with_workers()` for long-lived processes that should process jobs.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, false).await
    }

    /// Create a ServiceContext with in-process workers (lite mode only).
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
/// (default 60s; set to 0 to disable).
fn spawn_queue_summary_logger(jobs: Arc<dyn ServiceJobRuntime>) {
    let secs: u64 = std::env::var("AXON_QUEUE_SUMMARY_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    if secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(secs));
        // Skip the initial fire so the first log is one period in, not at startup.
        interval.tick().await;
        loop {
            interval.tick().await;
            let crawl = jobs.count_jobs(JobKind::Crawl).await.unwrap_or(0);
            let extract = jobs.count_jobs(JobKind::Extract).await.unwrap_or(0);
            let embed = jobs.count_jobs(JobKind::Embed).await.unwrap_or(0);
            let ingest = jobs.count_jobs(JobKind::Ingest).await.unwrap_or(0);
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
