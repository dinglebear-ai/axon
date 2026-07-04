use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::ids::*;
use super::lifecycle::JobDescriptor;
use super::stage::StageCounts;
use super::status::ProgressCurrent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCreateRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub job_kind: JobKind,
    pub job_intent: JobIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<WatchId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_job_id: Option<JobId>,
    pub priority: JobPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub stage_plan: Vec<JobStagePlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub auth_snapshot: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_snapshot_id: Option<ConfigSnapshotId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub requirements: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_schema: Option<String>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobStatusUpdate {
    pub job_id: JobId,
    pub status: LifecycleStatus,
    pub phase: PipelinePhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<StageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<StageCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ProgressCurrent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobHeartbeat {
    pub job_id: JobId,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub phase: PipelinePhase,
    pub status: LifecycleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<StageId>,
    pub heartbeat_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<StageCounts>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_reservations: Vec<ProviderReservationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderReservationSnapshot {
    pub reservation_id: ReservationId,
    pub provider_kind: ProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<ProviderId>,
    pub priority: JobPriority,
    pub requested_units: u32,
    pub granted_units: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    pub status: ProviderReservationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooling: Option<ProviderCoolingSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderCoolingSnapshot {
    pub reason: String,
    pub started_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<Timestamp>,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCancelRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCancelResult {
    pub job_id: JobId,
    pub status: LifecycleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canceled_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRetryRequest {
    pub mode: JobRetryMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_phase: Option<PipelinePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub overrides: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRetryResult {
    pub original_job_id: JobId,
    pub retry_job: JobDescriptor,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRecoveryRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<JobKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_before: Option<Timestamp>,
    pub limit: Option<u32>,
    #[serde(skip)]
    pub older_than_seconds: Option<u64>,
    #[serde(skip)]
    pub dry_run: bool,
    #[serde(skip)]
    pub allow_without_cutoff: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRecoveryResult {
    pub recovered: u64,
    pub job_ids: Vec<JobId>,
    pub warnings: Vec<SourceWarning>,
    #[serde(skip)]
    pub jobs_scanned: u64,
    #[serde(skip)]
    pub jobs_requeued: u64,
    #[serde(skip)]
    pub jobs_failed: u64,
}

pub type JobRecoverRequest = JobRecoveryRequest;
pub type JobRecoverResult = JobRecoveryResult;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCleanupRequest {
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<JobKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older_than: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<LifecycleStatus>,
    pub limit: Option<u32>,
    #[serde(skip)]
    pub older_than_seconds: Option<u64>,
    #[serde(skip)]
    pub confirm_all_terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCleanupResult {
    pub matched: u64,
    pub deleted: u64,
    pub dry_run: bool,
    pub warnings: Vec<SourceWarning>,
    #[serde(skip)]
    pub jobs_pruned: u64,
    #[serde(skip)]
    pub events_pruned: u64,
    #[serde(skip)]
    pub heartbeats_pruned: u64,
    #[serde(skip)]
    pub artifacts_pruned: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobClearRequest {
    pub status: LifecycleStatus,
    pub confirm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<JobKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older_than: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobClearResult {
    pub deleted: u64,
    pub status: LifecycleStatus,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobArtifactListRequest {
    pub job_id: JobId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ArtifactKind>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobArtifactListResult {
    pub artifacts: Vec<ArtifactRef>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
