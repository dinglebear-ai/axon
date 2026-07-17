//! Tracing-backed production observability sink.

pub const MODULE_NAME: &str = "sink::tracing_sink";

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{JobHeartbeat, SourceProgressEvent};

use crate::collector::{ObservabilitySink, Result};
use crate::metric::MetricSample;
use crate::redaction::redact_event;
use crate::sequence::SequenceRegistry;
use crate::span::SpanFieldSet;

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
        let write = redact_event(event).map_err(|error| *error)?;
        event = write.payload;
        event.sequence = self.sequences.next(event.job_id);
        // Bounded identifier/count/severity fields come from the shared
        // `SpanFieldSet` convention (see `crate::span`) instead of being
        // hardcoded here; only transport-envelope fields (status, visibility,
        // event_id, message) are read straight off the event.
        let fields = SpanFieldSet::from_event(&event);
        tracing::info!(
            job_id = fields.job_id.map(|id| id.0.to_string()).unwrap_or_default(),
            sequence = event.sequence,
            phase = fields.phase.map(crate::phase::label).unwrap_or_default(),
            status = enum_str(&event.status),
            severity = fields.severity.map(|s| enum_str(&s)).unwrap_or_default(),
            visibility = enum_str(&event.visibility),
            source_id = fields.source_id.map(|id| id.0).unwrap_or_default(),
            source_item_key = fields.source_item_key.map(|id| id.0).unwrap_or_default(),
            source_generation = fields.generation.map(|id| id.0).unwrap_or_default(),
            canonical_uri = fields.canonical_uri.unwrap_or_default(),
            document_id = fields.document_id.map(|id| id.0).unwrap_or_default(),
            chunk_id = fields.chunk_id.map(|id| id.0).unwrap_or_default(),
            adapter = fields.adapter.unwrap_or_default(),
            provider_id = fields.provider_id.map(|id| id.0).unwrap_or_default(),
            error_code = fields.error_code.unwrap_or_default(),
            event_id = %event.event_id,
            message = %event.message,
            redaction_status = ?write.redaction.redaction_status,
            redaction_version = %write.redaction.redaction_version,
            redacted_field_count = write.redaction.redacted_field_count,
            dropped_field_count = write.redaction.dropped_field_count,
            detector_count = write.redaction.detector_count,
            "observe.event"
        );
        Ok(())
    }

    async fn heartbeat(&self, mut heartbeat: JobHeartbeat) -> Result<()> {
        if heartbeat.last_event_sequence.is_none() {
            heartbeat.last_event_sequence = self.sequences.last(heartbeat.job_id);
        }
        let fields = SpanFieldSet::from_heartbeat(&heartbeat);
        tracing::debug!(
            job_id = fields.job_id.map(|id| id.0.to_string()).unwrap_or_default(),
            attempt = heartbeat.attempt,
            phase = fields.phase.map(crate::phase::label).unwrap_or_default(),
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
