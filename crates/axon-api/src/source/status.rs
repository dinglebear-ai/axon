use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::ids::*;
use super::stage::StageCounts;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SuccessEnvelope<T> {
    pub ok: bool,
    pub contract_version: String,
    pub data: T,
    pub warnings: Vec<SourceWarning>,
    pub request_id: String,
    pub trace: TraceContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PageInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<super::lifecycle::JobDescriptor>,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorEnvelope {
    pub ok: bool,
    pub contract_version: String,
    pub error: ApiError,
    pub request_id: String,
    pub trace: TraceContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TraceContext {
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub sampled: bool,
    pub attributes: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PageInfo {
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

// `ApiError` and its error enums are owned by `axon-error`, the lowest shared
// error boundary. `axon-api` re-exports them so the surrounding envelopes
// (`ErrorEnvelope`, `StreamEvent::Error`, `SourceStatus.last_error`,
// `SourceProgressEvent.error`, `capability.rs`) embed the one shared shape.
pub use axon_error::{ApiError, ErrorSeverity, ErrorStage, ErrorVisibility};

/// `StreamEvent.kind` — the flat set of streaming event kinds shared by SSE
/// and MCP streaming, per `docs/pipeline-unification/schemas/event-schema.md`
/// ("StreamEvent Shape").
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Progress,
    Token,
    Citation,
    Artifact,
    Warning,
    Error,
    Final,
}

/// The contracted flat SSE/MCP streaming envelope. `data` carries the
/// kind-specific payload (a `SourceProgressEvent` for `progress`, the
/// route-specific result DTO for `final`, a `{ "text": ... }` object for
/// `token`, etc.) — see the contract doc for the per-kind validation rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamEvent {
    pub event_id: String,
    pub kind: StreamKind,
    pub sequence: u64,
    pub timestamp: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default = "default_stream_data")]
    pub data: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<SourceWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

fn default_stream_data() -> serde_json::Value {
    serde_json::json!({})
}

impl StreamEvent {
    /// Builds a new envelope of `kind` with a fresh `evt_`-prefixed
    /// `event_id` and the current time as `timestamp`. Callers own
    /// `sequence` (monotonic per stream) and `data` (kind-specific payload).
    pub fn new(kind: StreamKind, sequence: u64, data: serde_json::Value) -> Self {
        Self {
            event_id: format!("evt_{}", uuid::Uuid::new_v4()),
            kind,
            sequence,
            timestamp: Timestamp::from(chrono::Utc::now()),
            job_id: None,
            request_id: None,
            data,
            warning: None,
            error: None,
        }
    }

    pub fn with_job_id(mut self, job_id: JobId) -> Self {
        self.job_id = Some(job_id);
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// `progress` kind carrying a `SourceProgressEvent` as `data`.
    pub fn progress(sequence: u64, event: &SourceProgressEvent) -> Self {
        Self::new(
            StreamKind::Progress,
            sequence,
            serde_json::to_value(event).unwrap_or_else(|_| serde_json::json!({})),
        )
    }

    /// `token` kind carrying `{ "text": <text> }` as `data`.
    pub fn token(sequence: u64, text: impl Into<String>) -> Self {
        Self::new(
            StreamKind::Token,
            sequence,
            serde_json::json!({ "text": text.into() }),
        )
    }

    /// `final` kind carrying the route-specific result DTO as `data`.
    pub fn final_event<T: Serialize>(sequence: u64, result: &T) -> Self {
        Self::new(
            StreamKind::Final,
            sequence,
            serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})),
        )
    }

    /// `error` kind. `data` carries only transport-safe diagnostic metadata
    /// per the contract; the full `ApiError` lives in `error`.
    pub fn error_event(sequence: u64, error: ApiError) -> Self {
        let mut event = Self::new(StreamKind::Error, sequence, serde_json::json!({}));
        event.error = Some(error);
        event
    }

    /// `warning` kind.
    pub fn warning_event(sequence: u64, warning: SourceWarning) -> Self {
        let mut event = Self::new(StreamKind::Warning, sequence, serde_json::json!({}));
        event.warning = Some(warning);
        event
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceStatus {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub status: LifecycleStatus,
    pub phase: PipelinePhase,
    pub heartbeat_at: Timestamp,
    pub counts: StageCounts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ProgressCurrent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ApiError>,
    pub warnings: Vec<SourceWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceProgressEvent {
    pub event_id: String,
    pub sequence: u64,
    pub job_id: JobId,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<StageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<BatchId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation_id: Option<ReservationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<CheckpointId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    pub phase: PipelinePhase,
    pub status: LifecycleStatus,
    pub severity: Severity,
    pub visibility: Visibility,
    pub message: String,
    pub timestamp: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<AdapterRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
    pub counts: StageCounts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ProgressTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ProgressCurrent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput: Option<ProgressThroughput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<SourceWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

pub const MAX_PROGRESS_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_PROGRESS_MESSAGE_BYTES: usize = 4 * 1024;
pub const MAX_PROGRESS_DEDUPE_KEY_BYTES: usize = 512;

impl SourceProgressEvent {
    /// Rejects progress payloads that would create unbounded durable rows or
    /// transport frames. Producers should put large diagnostics in artifacts.
    pub fn validate_bounds(&self) -> Result<(), Box<ApiError>> {
        if self.message.len() > MAX_PROGRESS_MESSAGE_BYTES {
            return Err(Box::new(progress_too_large("message", self.message.len())));
        }
        if let Some(key) = self.dedupe_key.as_deref()
            && key.len() > MAX_PROGRESS_DEDUPE_KEY_BYTES
        {
            return Err(Box::new(progress_too_large("dedupe_key", key.len())));
        }
        let encoded_bytes = serde_json::to_vec(self)
            .map_err(|error| {
                Box::new(ApiError::new(
                    "job_event.serialization_failed",
                    ErrorStage::Publishing,
                    error.to_string(),
                ))
            })?
            .len();
        if encoded_bytes > MAX_PROGRESS_EVENT_BYTES {
            return Err(Box::new(progress_too_large("event", encoded_bytes)));
        }
        Ok(())
    }
}

fn progress_too_large(field: &str, bytes: usize) -> ApiError {
    ApiError::new(
        "job_event.too_large",
        ErrorStage::Publishing,
        format!("progress {field} is {bytes} bytes; store large output as an artifact"),
    )
}

impl SourceProgressEvent {
    /// Builds a minimal, otherwise-empty progress event for callers that
    /// only need phase/status/severity/message — e.g. non-job-backed
    /// synchronous streaming routes (`ask`/`chat`/`summarize`/`research`).
    pub fn minimal(
        job_id: JobId,
        sequence: u64,
        phase: PipelinePhase,
        status: LifecycleStatus,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            event_id: format!("evt_{}", uuid::Uuid::new_v4()),
            sequence,
            job_id,
            attempt: 0,
            stage_id: None,
            batch_id: None,
            reservation_id: None,
            checkpoint_id: None,
            dedupe_key: None,
            phase,
            status,
            severity,
            visibility: Visibility::Public,
            message: message.into(),
            timestamp: Timestamp::from(chrono::Utc::now()),
            source_id: None,
            canonical_uri: None,
            adapter: None,
            scope: None,
            generation: None,
            counts: StageCounts {
                items_total: None,
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            timing: None,
            current: None,
            throughput: None,
            retry: None,
            warning: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressCurrent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<ChunkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressTiming {
    pub started_at: Timestamp,
    pub updated_at: Timestamp,
    pub elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressThroughput {
    pub items_per_second: Option<f64>,
    pub bytes_per_second: Option<f64>,
    pub chunks_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RetryState {
    pub attempt: u32,
    pub max_attempts: Option<u32>,
    pub next_retry_at: Option<Timestamp>,
    pub reason: String,
}
