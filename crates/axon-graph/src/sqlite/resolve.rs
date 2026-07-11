//! Identifier → node resolution for the SQLite graph store.

use axon_api::source::{
    GraphEdge, GraphIdentifier, GraphNode, GraphNodeId, GraphResolveMatch, GraphResolveMiss,
    GraphResolveRequest, GraphResolveResult, SourceId,
};
use sqlx::SqlitePool;

use super::row::{edge_from_row, node_from_row};
use crate::error::graph_storage_error;

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

/// Resolve a batch of identifiers to their durable nodes.
///
/// An identifier resolves by, in priority order: explicit `node_id`,
/// `canonical_uri`, then `value` (treated as a stable key). Each is looked up in
/// the `graph_aliases` table populated at write time.
pub async fn resolve(
    pool: &SqlitePool,
    request: GraphResolveRequest,
) -> StoreResult<GraphResolveResult> {
    let mut resolved = Vec::new();
    let mut misses = Vec::new();

    for identifier in request.identifiers {
        match resolve_one(pool, &identifier).await? {
            Some(node) => {
                let node_id = node.node_id.clone();
                let edges = if request.include_edges {
                    edges_for_node(pool, &node_id).await?
                } else {
                    Vec::new()
                };
                resolved.push(GraphResolveMatch {
                    identifier,
                    node,
                    confidence: 1.0,
                    evidence: Vec::new(),
                    edges,
                });
            }
            None => misses.push(GraphResolveMiss {
                identifier,
                reason: "no node matches identifier".to_string(),
            }),
        }
    }

    Ok(GraphResolveResult {
        resolved,
        misses,
        warnings: Vec::new(),
    })
}

/// Resolve a single identifier to its node, if any.
pub async fn resolve_one(
    pool: &SqlitePool,
    identifier: &GraphIdentifier,
) -> StoreResult<Option<GraphNode>> {
    let candidates: Vec<(&str, String)> = [
        identifier
            .node_id
            .as_ref()
            .map(|id| ("node_id", id.0.clone())),
        identifier
            .canonical_uri
            .as_ref()
            .map(|uri| ("canonical_uri", uri.clone())),
        identifier.value.as_ref().map(|v| ("stable_key", v.clone())),
    ]
    .into_iter()
    .flatten()
    .collect();

    for (alias_kind, alias_value) in candidates {
        if let Some(node_id) = alias_lookup(pool, alias_kind, &alias_value).await? {
            return node_by_id(pool, &node_id).await;
        }
    }
    Ok(None)
}

/// Look up a node id by an alias (kind, value) pair.
pub async fn alias_lookup(
    pool: &SqlitePool,
    alias_kind: &str,
    alias_value: &str,
) -> StoreResult<Option<GraphNodeId>> {
    use sqlx::Row;
    let row =
        sqlx::query("SELECT node_id FROM graph_aliases WHERE alias_kind = ? AND alias_value = ?")
            .bind(alias_kind)
            .bind(alias_value)
            .fetch_optional(pool)
            .await
            .map_err(|e| graph_storage_error(format!("failed alias lookup: {e}")))?;
    Ok(row.map(|r| GraphNodeId::new(r.get::<String, _>("node_id"))))
}

/// Fetch a node by its id.
pub async fn node_by_id(
    pool: &SqlitePool,
    node_id: &GraphNodeId,
) -> StoreResult<Option<GraphNode>> {
    let row = sqlx::query("SELECT * FROM graph_nodes WHERE node_id = ?")
        .bind(&node_id.0)
        .fetch_optional(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to fetch node: {e}")))?;
    match row {
        Some(row) => Ok(Some(node_from_row(&row)?)),
        None => Ok(None),
    }
}

/// All edges incident to a node (both directions), with evidence loaded.
pub async fn edges_for_node(
    pool: &SqlitePool,
    node_id: &GraphNodeId,
) -> StoreResult<Vec<GraphEdge>> {
    let rows = sqlx::query(
        "SELECT * FROM graph_edges WHERE from_node_id = ? OR to_node_id = ? ORDER BY edge_id",
    )
    .bind(&node_id.0)
    .bind(&node_id.0)
    .fetch_all(pool)
    .await
    .map_err(|e| graph_storage_error(format!("failed to fetch edges: {e}")))?;

    let mut edges = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut edge = edge_from_row(row)?;
        edge.evidence = super::query::evidence_for_edge(pool, &edge.edge_id.0).await?;
        edges.push(edge);
    }
    Ok(edges)
}

/// All nodes whose `source_ids_json` column contains `source_id`.
///
/// Matches on the exact quoted JSON string form (`"<source_id>"`) so a
/// source id that happens to be a substring of another does not falsely
/// match; this is a `LIKE` scan rather than a normalized join table, which is
/// an acceptable tradeoff for the current node volume (tracked as a
/// follow-up if `graph_nodes` needs a dedicated source-id index table).
pub async fn nodes_for_source(
    pool: &SqlitePool,
    source_id: &SourceId,
) -> StoreResult<Vec<GraphNode>> {
    let pattern = format!("%\"{}\"%", source_id.0);
    let rows =
        sqlx::query("SELECT * FROM graph_nodes WHERE source_ids_json LIKE ? ORDER BY node_id")
            .bind(pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| graph_storage_error(format!("failed to fetch nodes for source: {e}")))?;
    rows.iter().map(node_from_row).collect()
}
