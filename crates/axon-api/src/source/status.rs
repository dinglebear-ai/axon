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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamEvent {
    Progress { event: SourceProgressEvent },
    Result { result: SourceResultRef },
    Error { error: ApiError },
    Heartbeat { timestamp: Timestamp },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceResultRef {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub status: LifecycleStatus,
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
