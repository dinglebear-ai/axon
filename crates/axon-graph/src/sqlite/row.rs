//! Row (de)serialization helpers for the SQLite graph store.

use axon_api::source::{
    AuthorityLevel, ChunkId, DocumentId, GraphEdge, GraphEdgeId, GraphEvidence, GraphNode,
    GraphNodeId, MetadataMap, SourceId, SourceItemKey, SourceRange, Timestamp,
};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::error::graph_storage_error;

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

/// Serialize an [`AuthorityLevel`] to its stored string form.
pub fn authority_to_str(level: AuthorityLevel) -> &'static str {
    match level {
        AuthorityLevel::Official => "official",
        AuthorityLevel::Verified => "verified",
        AuthorityLevel::UserPinned => "user_pinned",
        AuthorityLevel::Inferred => "inferred",
        AuthorityLevel::Community => "community",
        AuthorityLevel::Mirror => "mirror",
        AuthorityLevel::Conflicting => "conflicting",
        AuthorityLevel::Unknown => "unknown",
    }
}

/// Parse a stored authority string back into an [`AuthorityLevel`].
pub fn authority_from_str(value: &str) -> AuthorityLevel {
    match value {
        "official" => AuthorityLevel::Official,
        "verified" => AuthorityLevel::Verified,
        "user_pinned" => AuthorityLevel::UserPinned,
        "inferred" => AuthorityLevel::Inferred,
        "community" => AuthorityLevel::Community,
        "mirror" => AuthorityLevel::Mirror,
        "conflicting" => AuthorityLevel::Conflicting,
        _ => AuthorityLevel::Unknown,
    }
}

/// Serialize a [`MetadataMap`] to a JSON string column.
pub fn metadata_to_json(map: &MetadataMap) -> StoreResult<String> {
    serde_json::to_string(&map.0)
        .map_err(|e| graph_storage_error(format!("failed to serialize metadata: {e}")))
}

/// Deserialize a JSON string column into a [`MetadataMap`].
pub fn metadata_from_json(raw: &str) -> StoreResult<MetadataMap> {
    let inner = serde_json::from_str(raw)
        .map_err(|e| graph_storage_error(format!("failed to deserialize metadata: {e}")))?;
    Ok(MetadataMap(inner))
}

/// Serialize a list of [`SourceId`] to a JSON string column.
pub fn source_ids_to_json(ids: &[SourceId]) -> StoreResult<String> {
    let raw: Vec<&str> = ids.iter().map(|id| id.0.as_str()).collect();
    serde_json::to_string(&raw)
        .map_err(|e| graph_storage_error(format!("failed to serialize source ids: {e}")))
}

/// Deserialize a JSON string column into a list of [`SourceId`].
pub fn source_ids_from_json(raw: &str) -> StoreResult<Vec<SourceId>> {
    let ids: Vec<String> = serde_json::from_str(raw)
        .map_err(|e| graph_storage_error(format!("failed to deserialize source ids: {e}")))?;
    Ok(ids.into_iter().map(SourceId::new).collect())
}

/// Reconstruct a [`GraphNode`] from a `graph_nodes` row.
pub fn node_from_row(row: &SqliteRow) -> StoreResult<GraphNode> {
    let metadata_raw: String = row.get("metadata_json");
    let source_ids_raw: String = row.get("source_ids_json");
    let authority_raw: String = row.get("authority");
    Ok(GraphNode {
        node_id: GraphNodeId::new(row.get::<String, _>("node_id")),
        kind: row.get("kind"),
        canonical_uri: row.get("canonical_uri"),
        display_name: row.get("display_name"),
        authority: authority_from_str(&authority_raw),
        confidence: row.get::<f64, _>("confidence") as f32,
        metadata: metadata_from_json(&metadata_raw)?,
        source_ids: source_ids_from_json(&source_ids_raw)?,
        created_at: Some(Timestamp(row.get("created_at"))),
        updated_at: Some(Timestamp(row.get("updated_at"))),
    })
}

/// Reconstruct a [`GraphEdge`] (without evidence) from a `graph_edges` row.
pub fn edge_from_row(row: &SqliteRow) -> StoreResult<GraphEdge> {
    let metadata_raw: String = row.get("metadata_json");
    let authority_raw: String = row.get("authority");
    Ok(GraphEdge {
        edge_id: GraphEdgeId::new(row.get::<String, _>("edge_id")),
        kind: row.get("kind"),
        from_node_id: GraphNodeId::new(row.get::<String, _>("from_node_id")),
        to_node_id: GraphNodeId::new(row.get::<String, _>("to_node_id")),
        authority: authority_from_str(&authority_raw),
        confidence: row.get::<f64, _>("confidence") as f32,
        evidence: Vec::new(),
        metadata: metadata_from_json(&metadata_raw)?,
    })
}

/// Reconstruct a [`GraphEvidence`] from a `graph_evidence` row.
pub fn evidence_from_row(row: &SqliteRow) -> StoreResult<GraphEvidence> {
    let range_raw: Option<String> = row.get("range_json");
    let range: Option<SourceRange> = match range_raw {
        Some(raw) => Some(
            serde_json::from_str(&raw)
                .map_err(|e| graph_storage_error(format!("failed to deserialize range: {e}")))?,
        ),
        None => None,
    };
    let metadata_raw: String = row.get("metadata_json");
    Ok(GraphEvidence {
        evidence_id: row.get("evidence_id"),
        evidence_kind: row.get("evidence_kind"),
        source_id: SourceId::new(row.get::<String, _>("source_id")),
        source_item_key: SourceItemKey::new(row.get::<String, _>("source_item_key")),
        document_id: row
            .get::<Option<String>, _>("document_id")
            .map(DocumentId::new),
        chunk_id: row.get::<Option<String>, _>("chunk_id").map(ChunkId::new),
        range,
        quote: row.get("quote"),
        confidence: row.get::<f64, _>("confidence") as f32,
        metadata: metadata_from_json(&metadata_raw)?,
    })
}

/// Serialize a [`SourceRange`] to an optional JSON string column.
pub fn range_to_json(range: &Option<SourceRange>) -> StoreResult<Option<String>> {
    match range {
        Some(range) => Ok(Some(serde_json::to_string(range).map_err(|e| {
            graph_storage_error(format!("failed to serialize range: {e}"))
        })?)),
        None => Ok(None),
    }
}
