use std::sync::Arc;

use crate::crates::core::config::Config;
use crate::crates::services::runtime::{ServiceJobRuntime, resolve_runtime_with_workers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityState {
    pub supported: bool,
    pub reason: Option<&'static str>,
}

impl CapabilityState {
    pub const fn supported() -> Self {
        Self {
            supported: true,
            reason: None,
        }
    }

    pub const fn unsupported(reason: &'static str) -> Self {
        Self {
            supported: false,
            reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCapabilities {
    pub export: CapabilityState,
    pub graph: CapabilityState,
    pub refresh_schedule: CapabilityState,
    pub watch_scheduler: CapabilityState,
}

impl ServiceCapabilities {
    fn from_config(cfg: &Config) -> Self {
        if cfg.lite_mode {
            return Self {
                export: CapabilityState::unsupported(
                    "export requires full history sources in the current implementation",
                ),
                graph: CapabilityState::unsupported("graph requires Postgres-backed graph storage"),
                refresh_schedule: CapabilityState::unsupported(
                    "refresh scheduling is not available in lite mode",
                ),
                watch_scheduler: CapabilityState::unsupported(
                    "watch scheduler is not available in lite mode",
                ),
            };
        }

        Self {
            export: CapabilityState::supported(),
            graph: CapabilityState::supported(),
            refresh_schedule: CapabilityState::supported(),
            watch_scheduler: CapabilityState::supported(),
        }
    }
}

#[derive(Clone)]
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub capabilities: ServiceCapabilities,
    pub jobs: Arc<dyn ServiceJobRuntime>,
}

impl ServiceContext {
    async fn build(
        cfg: Arc<Config>,
        spawn_workers: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let capabilities = ServiceCapabilities::from_config(cfg.as_ref());
        let jobs = resolve_runtime_with_workers(Arc::clone(&cfg), spawn_workers).await?;
        Ok(Self {
            cfg,
            capabilities,
            jobs,
        })
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

    pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        let capabilities = ServiceCapabilities::from_config(cfg.as_ref());
        Self {
            cfg,
            capabilities,
            jobs,
        }
    }

    pub fn with_jobs_runtime(mut self, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        self.jobs = jobs;
        self
    }
}
