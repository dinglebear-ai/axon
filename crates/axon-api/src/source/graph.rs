use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{SourceRange, SourceWarning};
use super::enums::AuthorityLevel;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceParseFacts {
    pub document_id: DocumentId,
    pub source_item_key: SourceItemKey,
    pub fact_kind: String,
    pub name: String,
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    pub confidence: f32,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphCandidate {
    pub candidate_id: String,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub item_canonical_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    pub producer: GraphCandidateProducer,
    pub nodes: Vec<GraphNodeCandidate>,
    pub edges: Vec<GraphEdgeCandidate>,
    pub evidence: Vec<GraphEvidence>,
    pub confidence: f32,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphCandidateProducer {
    pub adapter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parser: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphNodeCandidate {
    pub node_kind: String,
    pub stable_key: String,
    pub label: String,
    pub properties: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphEdgeCandidate {
    pub edge_kind: String,
    pub from_stable_key: String,
    pub to_stable_key: String,
    pub properties: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphEvidence {
    pub evidence_id: String,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    pub quote: Option<String>,
    pub confidence: f32,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<GraphEdgeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphNode {
    pub node_id: GraphNodeId,
    pub kind: String,
    pub canonical_uri: String,
    pub display_name: String,
    pub authority: AuthorityLevel,
    pub confidence: f32,
    pub metadata: MetadataMap,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphEdge {
    pub edge_id: GraphEdgeId,
    pub kind: String,
    pub from_node_id: GraphNodeId,
    pub to_node_id: GraphNodeId,
    pub authority: AuthorityLevel,
    pub confidence: f32,
    pub evidence: Vec<GraphEvidence>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphKindDocument {
    pub node_kinds: Vec<String>,
    pub edge_kinds: Vec<String>,
    pub evidence_kinds: Vec<String>,
    pub authority_levels: Vec<AuthorityLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphIdentifier {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphResolveRequest {
    pub identifiers: Vec<GraphIdentifier>,
    #[serde(default)]
    pub include_edges: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphResolveMatch {
    pub identifier: GraphIdentifier,
    pub node: GraphNode,
    pub confidence: f32,
    pub evidence: Vec<GraphEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphResolveMiss {
    pub identifier: GraphIdentifier,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphResolveResult {
    pub resolved: Vec<GraphResolveMatch>,
    pub misses: Vec<GraphResolveMiss>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GraphDirection {
    In,
    Out,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphQueryFilters {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphQueryRequest {
    pub start: GraphIdentifier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<String>,
    pub direction: GraphDirection,
    pub depth: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<GraphQueryFilters>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphQueryResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub evidence: Vec<GraphEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub warnings: Vec<SourceWarning>,
}
