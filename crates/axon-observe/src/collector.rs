//! Shared observability sink boundary.

pub const MODULE_NAME: &str = "collector";

use async_trait::async_trait;
use axon_api::source::{ApiError, JobHeartbeat, SourceProgressEvent};

use crate::metric::MetricSample;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait ObservabilitySink: Send + Sync {
    async fn emit(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
    async fn metric(&self, metric: MetricSample) -> Result<()>;
    async fn flush(&self) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg(test)]
pub struct NoopObservabilitySink;

#[async_trait]
#[cfg(test)]
impl ObservabilitySink for NoopObservabilitySink {
    async fn emit(&self, _event: SourceProgressEvent) -> Result<()> {
        Ok(())
    }

    async fn heartbeat(&self, _heartbeat: JobHeartbeat) -> Result<()> {
        Ok(())
    }

    async fn metric(&self, _metric: MetricSample) -> Result<()> {
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}
