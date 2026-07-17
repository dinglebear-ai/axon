use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::ids::*;
use super::job::JobHeartbeat;
use super::lifecycle::{JobDescriptor, SourceRequest, WatchSchedule};
use super::listing::Page;
use super::stage::StageCounts;
use super::status::{ApiError, ProgressCurrent, SourceStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobSummary {
    pub job_id: JobId,
    pub kind: JobKind,
    pub status: LifecycleStatus,
    pub phase: PipelinePhase,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<WatchId>,
    #[serde(skip)]
    pub intent: Option<JobIntent>,
    #[serde(skip)]
    pub started_at: Option<Timestamp>,
    #[serde(skip)]
    pub finished_at: Option<Timestamp>,
    #[serde(skip)]
    pub parent_job_id: Option<JobId>,
    #[serde(skip)]
    pub root_job_id: Option<JobId>,
    #[serde(skip)]
    pub attempt: u32,
    #[serde(skip, default = "default_job_priority")]
    pub priority: JobPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<StageCounts>,
    #[serde(skip)]
    pub current: Option<ProgressCurrent>,
    #[serde(skip)]
    pub heartbeat: Option<JobHeartbeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
    #[serde(skip)]
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobDetail {
    pub summary: JobSummary,
    pub request: Option<SourceRequest>,
    pub progress: Option<SourceStatus>,
    pub events: Page<JobEvent>,
    pub artifacts: Vec<ArtifactRef>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobEvent {
    pub event_id: String,
    pub sequence: u64,
    pub job_id: JobId,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<StageId>,
    pub phase: PipelinePhase,
    pub status: LifecycleStatus,
    pub severity: Severity,
    #[serde(default = "default_visibility")]
    pub visibility: Visibility,
    pub message: String,
    pub timestamp: Timestamp,
    pub details: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobListRequest {
    pub status: Option<LifecycleStatus>,
    pub kind: Option<JobKind>,
    pub source_id: Option<SourceId>,
    pub watch_id: Option<WatchId>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobControlAction {
    Cancel,
    Retry,
    Recover,
    ClearCompleted,
    ClearFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobControlRequest {
    pub action: JobControlAction,
    pub reason: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobEventListRequest {
    pub job_id: JobId,
    pub after_sequence: Option<u64>,
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PipelinePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobEventPage {
    pub events: Vec<JobEvent>,
    pub next_cursor: Option<String>,
    pub last_sequence: u64,
    #[serde(default, skip_serializing_if = "is_default_u32")]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceJobStatus {
    pub summary: JobSummary,
    pub attempts: Vec<JobAttemptSnapshot>,
    pub stages: Vec<JobStageSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_event_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobAttemptSnapshot {
    pub attempt: u32,
    pub status: LifecycleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub started_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobStageSnapshot {
    pub stage_id: StageId,
    pub phase: PipelinePhase,
    pub status: LifecycleStatus,
    pub required: bool,
    pub provider_requirements: Vec<ProviderRequirement>,
    pub counts: StageCounts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub source: String,
    pub schedule: WatchSchedule,
    #[serde(default = "default_true")]
    pub embed: bool,
    pub options: AdapterOptions,
    pub scope: Option<SourceScope>,
    pub collection: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchSummary {
    pub watch_id: WatchId,
    pub source_id: SourceId,
    pub enabled: bool,
    pub schedule: WatchSchedule,
    pub next_run_at: Timestamp,
    pub last_job_id: Option<JobId>,
    pub last_status: Option<LifecycleStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchDetail {
    pub summary: WatchSummary,
    pub request: WatchRequest,
    pub recent_jobs: Vec<JobSummary>,
    pub history: Page<WatchHistoryEntry>,
    pub artifacts: Vec<WatchArtifactSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchListRequest {
    pub enabled: Option<bool>,
    pub source_id: Option<SourceId>,
    pub adapter: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchUpdateRequest {
    pub enabled: Option<bool>,
    pub schedule: Option<WatchSchedule>,
    pub options: Option<AdapterOptions>,
    pub embed: Option<bool>,
    pub collection: Option<String>,
    #[serde(skip)]
    pub scope: Option<SourceScope>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WatchControlAction {
    Pause,
    Resume,
    Delete,
    RunNow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchControlRequest {
    pub action: WatchControlAction,
    pub reason: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchExecRequest {
    pub reason: Option<String>,
    pub refresh: Option<SourceRefreshPolicy>,
    pub wait: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchHistoryEntry {
    pub job_id: JobId,
    pub watch_id: WatchId,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub status: LifecycleStatus,
    pub counts: StageCounts,
    pub artifacts: Vec<ArtifactRef>,
    pub error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchArtifactSummary {
    pub artifact_id: ArtifactId,
    pub watch_id: WatchId,
    pub job_id: Option<JobId>,
    pub kind: ArtifactKind,
    pub uri: String,
    pub created_at: Timestamp,
    pub content_type: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchArtifactListRequest {
    pub watch_id: WatchId,
    pub kind: Option<ArtifactKind>,
    pub since: Option<Timestamp>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchDescriptor {
    pub watch_id: WatchId,
    pub source_id: SourceId,
    pub enabled: bool,
    pub schedule: WatchSchedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_job: Option<JobDescriptor>,
    pub warnings: Vec<SourceWarning>,
}

fn default_visibility() -> Visibility {
    Visibility::Internal
}

fn default_true() -> bool {
    true
}

fn default_job_priority() -> JobPriority {
    JobPriority::Normal
}

fn is_default_u32(value: &u32) -> bool {
    *value == 0
}
