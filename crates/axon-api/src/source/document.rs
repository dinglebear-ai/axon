use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::graph::*;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceDocument {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub canonical_uri: String,
    pub content_kind: ContentKind,
    pub content: ContentRef,
    pub metadata: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<ArtifactId>,
    pub chunk_hints: Vec<ChunkHint>,
    pub parser_hints: Vec<ParserHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedDocument {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub chunks: Vec<PreparedChunk>,
    pub metadata: MetadataMap,
    pub cleanup_keys: Vec<CleanupKey>,
    pub graph_refs: Vec<GraphRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedChunk {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub chunk_index: u32,
    pub chunk_text: String,
    pub chunk_hash: String,
    pub chunk_locator: ChunkLocator,
    pub source_range: SourceRange,
    pub content_kind: ContentKind,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkLocator {
    pub canonical_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub heading_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanupKey {
    pub kind: CleanupDebtKind,
    pub selector: CleanupSelector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CleanupSelector {
    Source {
        source_id: SourceId,
    },
    Generation {
        source_id: SourceId,
        generation: SourceGenerationId,
    },
    SourceItem {
        source_id: SourceId,
        source_item_key: SourceItemKey,
        generation: SourceGenerationId,
    },
    Document {
        document_id: DocumentId,
    },
    Chunk {
        chunk_id: ChunkId,
    },
    Artifact {
        artifact_id: ArtifactId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentStatus {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub status: DocumentLifecycleStatus,
    pub updated_at: Timestamp,
    pub chunk_count: u32,
    pub vector_point_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SourceError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_status: Option<LifecycleStatus>,
}
