//! In-memory observability fixtures for tests.

pub const MODULE_NAME: &str = "testing";

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{ApiError, JobHeartbeat, SourceProgressEvent};

use crate::collector::{ObservabilitySink, Result};
use crate::metric::MetricSample;

#[derive(Debug, Clone, Default)]
pub struct InMemoryObservabilitySink {
    state: Arc<Mutex<InMemoryObservabilityState>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InMemoryObservabilitySnapshot {
    pub events: Vec<SourceProgressEvent>,
    pub heartbeats: Vec<JobHeartbeat>,
    pub metrics: Vec<MetricSample>,
    pub operations: Vec<String>,
}

#[derive(Debug, Default)]
struct InMemoryObservabilityState {
    events: Vec<SourceProgressEvent>,
    heartbeats: Vec<JobHeartbeat>,
    metrics: Vec<MetricSample>,
    operations: Vec<String>,
}

impl InMemoryObservabilitySink {
    pub fn snapshot(&self) -> InMemoryObservabilitySnapshot {
        let state = self.state.lock().expect("observability state poisoned");
        InMemoryObservabilitySnapshot {
            events: state.events.clone(),
            heartbeats: state.heartbeats.clone(),
            metrics: state.metrics.clone(),
            operations: state.operations.clone(),
        }
    }
}

#[async_trait]
impl ObservabilitySink for InMemoryObservabilitySink {
    async fn emit(&self, event: SourceProgressEvent) -> Result<()> {
        let mut state = self.state.lock().expect("observability state poisoned");
        state.events.push(event);
        state.operations.push("emit".to_string());
        Ok(())
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        let mut state = self.state.lock().expect("observability state poisoned");
        state.heartbeats.push(heartbeat);
        state.operations.push("heartbeat".to_string());
        Ok(())
    }

    async fn metric(&self, metric: MetricSample) -> Result<()> {
        let mut state = self.state.lock().expect("observability state poisoned");
        state.metrics.push(metric);
        state.operations.push("metric".to_string());
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let mut state = self.state.lock().expect("observability state poisoned");
        state.operations.push("flush".to_string());
        Ok(())
    }
}

pub fn test_error(code: &str) -> ApiError {
    ApiError::new(code, axon_error::ErrorStage::Planning, code)
}
