//! Tracing-backed production observability sink.

pub const MODULE_NAME: &str = "sink::tracing_sink";

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{JobHeartbeat, SourceProgressEvent};

use crate::collector::{ObservabilitySink, Result};
use crate::metric::MetricSample;
use crate::sequence::SequenceRegistry;

/// Emits the shared event model to the `tracing` subscriber as structured,
/// redaction-safe fields. It carries only bounded/identifier fields (never raw
/// content) and stamps a monotonic per-`job_id` sequence at emit time.
#[derive(Clone, Default)]
pub struct TracingObservabilitySink {
    sequences: Arc<SequenceRegistry>,
}

impl TracingObservabilitySink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Share this sink's sequence registry (e.g. to keep a paired SQLite sink in
    /// lockstep when both observe the same stream).
    pub fn with_sequences(sequences: Arc<SequenceRegistry>) -> Self {
        Self { sequences }
    }

    pub fn sequences(&self) -> Arc<SequenceRegistry> {
        Arc::clone(&self.sequences)
    }
}

#[async_trait]
impl ObservabilitySink for TracingObservabilitySink {
    async fn emit(&self, mut event: SourceProgressEvent) -> Result<()> {
        event.sequence = self.sequences.next(event.job_id);
        tracing::info!(
            job_id = %event.job_id.0,
            sequence = event.sequence,
            phase = enum_str(&event.phase),
            status = enum_str(&event.status),
            severity = enum_str(&event.severity),
            visibility = enum_str(&event.visibility),
            event_id = %event.event_id,
            message = %event.message,
            "observe.event"
        );
        Ok(())
    }

    async fn heartbeat(&self, mut heartbeat: JobHeartbeat) -> Result<()> {
        if heartbeat.last_event_sequence.is_none() {
            heartbeat.last_event_sequence = self.sequences.last(heartbeat.job_id);
        }
        tracing::debug!(
            job_id = %heartbeat.job_id.0,
            attempt = heartbeat.attempt,
            phase = enum_str(&heartbeat.phase),
            status = enum_str(&heartbeat.status),
            last_event_sequence = heartbeat.last_event_sequence,
            "observe.heartbeat"
        );
        Ok(())
    }

    async fn metric(&self, metric: MetricSample) -> Result<()> {
        tracing::debug!(
            metric = %metric.name,
            value = metric.value,
            unit = metric.unit.as_deref().unwrap_or(""),
            "observe.metric"
        );
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

fn enum_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
#[path = "tracing_sink_tests.rs"]
mod tests;
