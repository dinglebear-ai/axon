use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatch {
    pub batch_id: BatchId,
    pub job_id: JobId,
    pub provider_id: ProviderId,
    pub model: String,
    pub items: Vec<EmbeddingInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    pub priority: JobPriority,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingInput {
    pub chunk_id: ChunkId,
    pub text: String,
    pub content_kind: ContentKind,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingResult {
    pub batch_id: BatchId,
    pub job_id: JobId,
    pub provider_id: ProviderId,
    pub model: String,
    pub dimensions: u32,
    pub vectors: Vec<EmbeddingVector>,
    pub usage: ProviderUsage,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingVector {
    pub chunk_id: ChunkId,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub requests: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorPointBatch {
    pub batch_id: BatchId,
    pub collection: String,
    pub points: Vec<VectorPoint>,
    pub model: String,
    pub dimensions: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_vectors: Option<Vec<SparseVector>>,
    pub payload_indexes: Vec<PayloadIndexSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorPoint {
    pub point_id: VectorPointId,
    pub chunk_id: ChunkId,
    pub vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_vector: Option<SparseVector>,
    pub payload: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SparseVector {
    pub chunk_id: ChunkId,
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PayloadIndexSpec {
    pub field_name: String,
    pub field_schema: PayloadFieldSchema,
    pub required_for_filters: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CollectionSpec {
    pub collection: String,
    pub dense: VectorConfig,
    pub payload_indexes: Vec<PayloadIndexSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse: Option<SparseVectorConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<VectorDistance>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub name: String,
    #[schemars(range(min = 1))]
    #[schema(minimum = 1)]
    pub dimensions: u32,
    pub distance: VectorDistance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SparseVectorConfig {
    pub name: String,
    pub modifier: SparseVectorModifier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VectorDeleteSelector {
    Source {
        collection: String,
        source_id: SourceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation: Option<SourceGenerationId>,
    },
    Generation {
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    },
    /// Delete every point in `collection`, keeping the (now-empty) collection
    /// itself. Distinct from `axon reset`, which also wipes SQLite/job state.
    Collection {
        collection: String,
    },
    Document {
        collection: String,
        document_id: DocumentId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation: Option<SourceGenerationId>,
    },
    Chunks {
        collection: String,
        chunk_ids: Vec<ChunkId>,
    },
    Points {
        collection: String,
        point_ids: Vec<VectorPointId>,
    },
    CanonicalUri {
        collection: String,
        canonical_uri: String,
        match_prefix: bool,
    },
    Filter {
        collection: String,
        filter: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorStoreDeleteResult {
    pub collection: String,
    pub points_matched: u64,
    pub points_deleted: u64,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorSearchRequest {
    pub collection: String,
    pub query: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dense_vector: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse_vector: Option<SparseVector>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub filters: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_refs: Vec<GraphNodeId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorSearchResult {
    pub collection: String,
    pub results: Vec<VectorSearchMatch>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorSearchMatch {
    pub point_id: VectorPointId,
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<ChunkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub payload: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadFieldSchema {
    Keyword,
    Integer,
    Float,
    Boolean,
    Datetime,
    Text,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VectorDistance {
    Cosine,
    Dot,
    Euclid,
    Manhattan,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparseVectorModifier {
    None,
    Idf,
}
