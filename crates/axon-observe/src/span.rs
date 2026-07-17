//! Tracing span-field conventions for the unified observability boundary.
//!
//! `SpanFieldSet` is the canonical bounded field set attached to pipeline
//! spans/log events, per the "Tracing" section of
//! `docs/pipeline-unification/runtime/observability-contract.md` (`job_id`,
//! `source_id`, `adapter`, `scope`, `phase`, `provider_id`, bounded counts,
//! error code/severity) and the `SpanFieldSet` `$def` required by
//! `docs/pipeline-unification/schemas/event-schema.md`.
//!
//! [`sink::tracing_sink::TracingObservabilitySink`](crate::sink::tracing_sink)
//! builds a `SpanFieldSet` from each `SourceProgressEvent`/`JobHeartbeat` via
//! [`SpanFieldSet::from_event`]/[`SpanFieldSet::from_heartbeat`] instead of
//! hardcoding its own ad hoc field list, so every tracing emission carries the
//! same shape this crate documents. Full content, secrets, raw prompts, and
//! raw tool output must never be attached as span attributes — only the
//! bounded identifier/count/severity fields below.

pub const MODULE_NAME: &str = "span";

use axon_api::source::{
    ApiError, ChunkId, DocumentId, JobHeartbeat, JobId, PipelinePhase, ProviderId, Severity,
    SourceGenerationId, SourceId, SourceItemKey, SourceProgressEvent, SourceScope, StageCounts,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The canonical bounded span/log field set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpanFieldSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<ChunkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PipelinePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<StageCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

impl SpanFieldSet {
    /// Derive the bounded span fields from a progress event.
    pub fn from_event(event: &SourceProgressEvent) -> Self {
        Self {
            job_id: Some(event.job_id),
            source_id: event.source_id.clone(),
            source_item_key: event
                .current
                .as_ref()
                .and_then(|current| current.source_item_key.clone()),
            generation: event.generation.clone(),
            canonical_uri: event.canonical_uri.clone(),
            document_id: event
                .current
                .as_ref()
                .and_then(|current| current.document_id.clone()),
            chunk_id: event
                .current
                .as_ref()
                .and_then(|current| current.chunk_id.clone()),
            adapter: event.adapter.as_ref().map(|adapter| adapter.name.clone()),
            scope: event.scope,
            phase: Some(event.phase),
            provider_id: event
                .current
                .as_ref()
                .and_then(|current| current.provider.clone()),
            counts: Some(event.counts.clone()),
            error_code: event.error.as_ref().map(error_code),
            severity: Some(event.severity),
        }
    }

    /// Derive the bounded span fields from a heartbeat.
    pub fn from_heartbeat(heartbeat: &JobHeartbeat) -> Self {
        Self {
            job_id: Some(heartbeat.job_id),
            source_id: None,
            source_item_key: None,
            generation: None,
            canonical_uri: None,
            document_id: None,
            chunk_id: None,
            adapter: None,
            scope: None,
            phase: Some(heartbeat.phase),
            provider_id: None,
            counts: heartbeat.counts.clone(),
            error_code: None,
            severity: None,
        }
    }
}

fn error_code(error: &ApiError) -> String {
    error.code.to_string()
}

#[cfg(test)]
#[path = "span_tests.rs"]
mod tests;
