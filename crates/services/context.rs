use std::sync::Arc;

use crate::crates::core::config::Config;
use crate::crates::services::runtime::{ServiceJobRuntime, resolve_runtime_with_workers};

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

    /// Factory for CLI command handlers — enqueue-only, no worker spawning.
    ///
    /// Fire-and-forget jobs (without --wait) require `axon mcp` to be running as a
    /// daemon. `axon mcp` uses `for_mcp()` which spawns in-process workers.
    pub async fn for_cli(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new(cfg).await
    }

    /// Factory for MCP server and long-running daemons — spawns in-process workers.
    ///
    /// `axon mcp` is the worker daemon. Fire-and-forget CLI jobs (without --wait)
    /// require `axon mcp` running to be processed.
    pub async fn for_mcp(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new_with_workers(cfg).await
    }

    pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        Self { cfg, jobs }
    }

    pub fn with_jobs_runtime(mut self, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        self.jobs = jobs;
        self
    }
}
