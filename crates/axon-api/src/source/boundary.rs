use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::capability::RenderMode;
use super::common::*;
use super::document::SourceDocument;
use super::enums::*;
use super::graph::*;
use super::ids::*;
use super::lifecycle::{JobDescriptor, SourceRequest};
use super::listing::{JobEvent, JobSummary, Page, WatchSummary};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Decision,
    Fact,
    Preference,
    Task,
    Bug,
    Working,
    Summary,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Superseded,
    Archived,
    Forgotten,
    Review,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryScope {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryDecayPolicy {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_life_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryLink {
    pub link_type: String,
    pub target: String,
    pub confidence: f32,
    pub evidence: Vec<GraphEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryHistoryEvent {
    pub status: MemoryStatus,
    pub message: String,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub memory_type: MemoryType,
    pub body: String,
    pub confidence: f32,
    pub salience: f32,
    pub scope: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MemoryLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<MemoryDecayPolicy>,
    #[serde(default)]
    pub embed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryResult {
    pub memory_id: MemoryId,
    pub memory_type: MemoryType,
    pub status: MemoryStatus,
    pub memory_score: f32,
    pub confidence: f32,
    pub salience: f32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vector_point_ids: Vec<VectorPointId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub memory_type: MemoryType,
    pub status: MemoryStatus,
    pub body: String,
    pub confidence: f32,
    pub salience: f32,
    pub scope: MemoryScope,
    pub history: Vec<MemoryHistoryEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MemoryLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<MemoryDecayPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedding_refs: Vec<VectorPointId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchRequest {
    pub query: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub filters: MetadataMap,
    #[serde(default)]
    pub include_graph: bool,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default)]
    pub reinforce: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchMatch {
    pub record: MemoryRecord,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchResult {
    pub results: Vec<MemorySearchMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_embedding_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphQueryResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryContextRequest {
    pub token_budget: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub filters: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(default)]
    pub include_working: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryContextResult {
    pub context: String,
    pub memories: Vec<MemoryRecord>,
    pub exclusions: Vec<String>,
    pub token_estimate: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryLinkRequest {
    pub memory_id: MemoryId,
    pub link: MemoryLink,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryReinforcement {
    pub amount: f32,
    pub reason: String,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactWriteRequest {
    pub kind: ArtifactKind,
    pub content_type: String,
    pub content: ContentRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactHandle {
    pub artifact_id: ArtifactId,
    pub artifact_kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReadResult {
    pub handle: ArtifactHandle,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentRef>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveConfig {
    pub snapshot_id: ConfigSnapshotId,
    pub values: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigValidationReport {
    pub valid: bool,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentCacheKey {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CachedDocument {
    pub document: SourceDocument,
    pub cached_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocumentCacheInvalidation {
    Source { source_id: SourceId },
    Generation { generation: SourceGenerationId },
    Key { key: DocumentCacheKey },
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobCreateRequest {
    pub kind: JobKind,
    pub request: SourceRequest,
    pub priority: JobPriority,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobStatusUpdate {
    pub job_id: JobId,
    pub status: LifecycleStatus,
    pub phase: PipelinePhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobHeartbeat {
    pub job_id: JobId,
    pub phase: PipelinePhase,
    pub timestamp: Timestamp,
}

pub type JobEventPage = Page<JobEvent>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchHistoryRequest {
    pub watch_id: WatchId,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchHistoryResult {
    pub runs: Vec<JobDescriptor>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    pub limit: u32,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub query: String,
    pub results: Vec<SearchResultItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResultItem {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchRequest {
    pub uri: String,
    pub method: String,
    pub headers: RedactedHeaders,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchedResource {
    pub uri: String,
    pub status: u16,
    pub content: ContentRef,
    pub headers: RedactedHeaders,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub uri: String,
    pub mode: RenderMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderedResource {
    pub uri: String,
    pub markdown: String,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkCaptureRequest {
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkCaptureResult {
    pub uri: String,
    pub entries: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialRequest {
    pub credential_kind: CredentialKind,
    pub secret_ref: SecretRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialMaterial {
    pub secret_ref: SecretRef,
    pub redacted_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RateLimitRequest {
    pub provider_id: ProviderId,
    pub units: u32,
    pub priority: JobPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RateLimitPermit {
    pub provider_id: ProviderId,
    pub units: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthProbeRequest {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityPolicyRequest {
    pub caller: CallerContext,
    pub safety_class: SafetyClass,
    pub target: String,
}

pub type JobSummaryPage = Page<JobSummary>;
pub type WatchSummaryPage = Page<WatchSummary>;
