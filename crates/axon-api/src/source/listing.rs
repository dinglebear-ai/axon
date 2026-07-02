use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::document::*;
use super::enums::*;
use super::graph::*;
use super::ids::*;
use super::job::JobHeartbeat;
use super::lifecycle::WatchSchedule;
use super::stage::{ManifestItem, StageCounts};
use super::status::{ApiError, ProgressCurrent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceSummary {
    pub source_id: SourceId,
    pub canonical_uri: String,
    pub display_name: String,
    pub source_kind: SourceKind,
    pub adapter: AdapterRef,
    pub authority: AuthorityLevel,
    pub status: LifecycleStatus,
    pub counts: SourceCounts,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<WatchId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_job_id: Option<JobId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceListRequest {
    pub source_kind: Option<SourceKind>,
    pub adapter: Option<String>,
    pub status: Option<LifecycleStatus>,
    pub authority: Option<AuthorityLevel>,
    pub watch_enabled: Option<bool>,
    pub tag: Option<String>,
    pub query: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceDetail {
    pub summary: SourceSummary,
    pub active_generation: Option<SourceGenerationId>,
    pub latest_generation: Option<SourceGenerationId>,
    pub items: Page<SourceItem>,
    pub documents: Page<DocumentSummary>,
    pub graph_refs: Vec<GraphRef>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceItem {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub status: DocumentLifecycleStatus,
    pub content_hash: String,
    pub generation: SourceGenerationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub document_ids: Vec<DocumentId>,
    pub graph_refs: Vec<GraphRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceItemDetail {
    pub item: SourceItem,
    pub manifest: ManifestItem,
    pub statuses: Vec<DocumentStatus>,
    pub errors: Vec<SourceError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentSummary {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub status: DocumentLifecycleStatus,
    pub chunk_count: u32,
    pub vector_point_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<ContentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub graph_refs: Vec<GraphRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentDetail {
    pub summary: DocumentSummary,
    pub generation: SourceGenerationId,
    pub metadata: MetadataMap,
    pub chunks: Vec<ChunkSummary>,
    pub vector_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkSummary {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub chunk_index: u32,
    pub chunk_locator: ChunkLocator,
    pub source_range: SourceRange,
    pub metadata: MetadataMap,
    pub graph_refs: Vec<GraphRef>,
    pub vector_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkDetail {
    pub summary: ChunkSummary,
    pub content_hash: String,
    pub content: Option<String>,
    pub payload: MetadataMap,
    pub embedding_metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobSummary {
    pub job_id: JobId,
    pub kind: JobKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<JobIntent>,
    pub status: LifecycleStatus,
    pub phase: PipelinePhase,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<WatchId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_job_id: Option<JobId>,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default = "default_job_priority")]
    pub priority: JobPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<StageCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<ProgressCurrent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<JobHeartbeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobDetail {
    pub summary: JobSummary,
    pub request: Option<super::lifecycle::SourceRequest>,
    pub progress: Option<super::status::SourceStatus>,
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
    pub phase: Option<PipelinePhase>,
    pub severity: Option<Severity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    pub since_sequence: Option<u64>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobEventPage {
    pub events: Vec<JobEvent>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
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

fn default_visibility() -> Visibility {
    Visibility::Internal
}

fn default_job_priority() -> JobPriority {
    JobPriority::Normal
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub source: String,
    pub schedule: WatchSchedule,
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
    pub schedule: Option<WatchSchedule>,
    pub enabled: Option<bool>,
    pub embed: Option<bool>,
    pub scope: Option<SourceScope>,
    pub options: Option<AdapterOptions>,
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
