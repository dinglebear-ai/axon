use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::document::*;
use super::enums::*;
use super::graph::*;
use super::ids::*;
use super::stage::ManifestItem;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub limit: u32,
    #[serde(skip)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<WatchId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_node_ids: Vec<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refreshed_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_label: Option<String>,
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
pub struct DomainListRequest {
    pub domain: Option<String>,
    pub source_kind: Option<SourceKind>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DomainSummary {
    pub domain: String,
    pub source_count: u64,
    pub document_count: u64,
    pub chunk_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_refresh_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_sources: Vec<SourceSummary>,
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
pub struct SourceGenerationSummary {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub status: LifecycleStatus,
    pub counts: SourceCounts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_status: Option<LifecycleStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceGenerationDetail {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub status: LifecycleStatus,
    pub counts: SourceCounts,
    pub items: Vec<SourceItem>,
    pub documents: Vec<DocumentSummary>,
    pub chunks: Vec<ChunkSummary>,
    pub cleanup_debt: Vec<super::state::CleanupDebt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SourceError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub document_ids: Vec<DocumentId>,
    pub graph_refs: Vec<GraphRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceItemDetail {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub status: DocumentLifecycleStatus,
    pub content_hash: String,
    pub generation: SourceGenerationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub document_ids: Vec<DocumentId>,
    pub graph_refs: Vec<GraphRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
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
    pub generation: SourceGenerationId,
    pub metadata: MetadataMap,
    pub chunk_summary: ChunkSummary,
    pub vector_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<ChunkSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph: Vec<GraphRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentListRequest {
    pub source_id: Option<SourceId>,
    pub status: Option<DocumentLifecycleStatus>,
    pub generation: Option<SourceGenerationId>,
    pub content_kind: Option<ContentKind>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
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
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub chunk_index: u32,
    pub chunk_locator: ChunkLocator,
    pub source_range: SourceRange,
    pub metadata: MetadataMap,
    pub graph_refs: Vec<GraphRef>,
    pub vector_refs: Vec<String>,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub payload: MetadataMap,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub embedding_metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkListRequest {
    pub document_id: DocumentId,
    pub include_content: Option<bool>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkGetRequest {
    pub document_id: DocumentId,
    pub chunk_id: ChunkId,
    pub include_content: Option<bool>,
}
