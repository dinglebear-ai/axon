//! Graph traversal query for the SQLite graph store.

use std::collections::{BTreeSet, VecDeque};

use axon_api::source::{
    GraphDirection, GraphEdge, GraphEvidence, GraphNode, GraphNodeId, GraphQueryRequest,
    GraphQueryResult,
};
use sqlx::SqlitePool;

use super::row::{edge_from_row, evidence_from_row};
use crate::error::graph_storage_error;

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

/// Breadth-first traversal from `request.start` up to `request.depth`,
/// following edges in `request.direction`, filtered by `request.edges`
/// (edge-kind allowlist) and `request.limit` (max edges returned).
pub async fn query(pool: &SqlitePool, request: GraphQueryRequest) -> StoreResult<GraphQueryResult> {
    let Some(start) = super::resolve::resolve_one(pool, &request.start).await? else {
        return Ok(empty_result());
    };

    let edge_filter: BTreeSet<String> = request.edges.iter().cloned().collect();
    let limit = usize::try_from(request.limit)
        .ok()
        .filter(|l| *l > 0)
        .unwrap_or(usize::MAX);

    let mut nodes: Vec<GraphNode> = vec![start.clone()];
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut seen_nodes: BTreeSet<String> = BTreeSet::from([start.node_id.0.clone()]);
    let mut seen_edges: BTreeSet<String> = BTreeSet::new();
    let mut frontier: VecDeque<(GraphNodeId, u32)> = VecDeque::from([(start.node_id, 0)]);

    while let Some((node_id, depth)) = frontier.pop_front() {
        if depth >= request.depth || edges.len() >= limit {
            continue;
        }
        let incident = incident_edges(pool, &node_id, request.direction).await?;
        for mut edge in incident {
            if !edge_filter.is_empty() && !edge_filter.contains(&edge.kind) {
                continue;
            }
            let next = next_node(&edge, &node_id, request.direction);
            if seen_edges.insert(edge.edge_id.0.clone()) {
                edge.evidence = evidence_for_edge(pool, &edge.edge_id.0).await?;
                edges.push(edge);
                if edges.len() >= limit {
                    break;
                }
            }
            if let Some(next_id) = next
                && seen_nodes.insert(next_id.0.clone())
            {
                if let Some(node) = super::resolve::node_by_id(pool, &next_id).await? {
                    nodes.push(node);
                }
                frontier.push_back((next_id, depth + 1));
            }
        }
    }

    let evidence: Vec<GraphEvidence> = edges.iter().flat_map(|e| e.evidence.clone()).collect();
    Ok(GraphQueryResult {
        nodes,
        edges,
        evidence,
        next_cursor: None,
        warnings: Vec::new(),
    })
}

/// Fetch edges incident to `node_id` in the requested direction.
async fn incident_edges(
    pool: &SqlitePool,
    node_id: &GraphNodeId,
    direction: GraphDirection,
) -> StoreResult<Vec<GraphEdge>> {
    let sql = match direction {
        GraphDirection::Out => "SELECT * FROM graph_edges WHERE from_node_id = ? ORDER BY edge_id",
        GraphDirection::In => "SELECT * FROM graph_edges WHERE to_node_id = ? ORDER BY edge_id",
        GraphDirection::Both => {
            "SELECT * FROM graph_edges WHERE from_node_id = ? OR to_node_id = ? ORDER BY edge_id"
        }
    };
    let mut q = sqlx::query(sql).bind(&node_id.0);
    if matches!(direction, GraphDirection::Both) {
        q = q.bind(&node_id.0);
    }
    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to fetch incident edges: {e}")))?;
    rows.iter().map(edge_from_row).collect()
}

/// The node on the far side of `edge` from `node_id`, per direction.
fn next_node(
    edge: &GraphEdge,
    node_id: &GraphNodeId,
    direction: GraphDirection,
) -> Option<GraphNodeId> {
    match direction {
        GraphDirection::Out if edge.from_node_id == *node_id => Some(edge.to_node_id.clone()),
        GraphDirection::In if edge.to_node_id == *node_id => Some(edge.from_node_id.clone()),
        GraphDirection::Both if edge.from_node_id == *node_id => Some(edge.to_node_id.clone()),
        GraphDirection::Both if edge.to_node_id == *node_id => Some(edge.from_node_id.clone()),
        _ => None,
    }
}

/// Load all evidence rows for an edge.
pub async fn evidence_for_edge(
    pool: &SqlitePool,
    edge_id: &str,
) -> StoreResult<Vec<GraphEvidence>> {
    let rows = sqlx::query("SELECT * FROM graph_evidence WHERE edge_id = ? ORDER BY evidence_id")
        .bind(edge_id)
        .fetch_all(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to fetch evidence: {e}")))?;
    rows.iter().map(evidence_from_row).collect()
}

fn empty_result() -> GraphQueryResult {
    GraphQueryResult {
        nodes: Vec::new(),
        edges: Vec::new(),
        evidence: Vec::new(),
        next_cursor: None,
        warnings: Vec::new(),
    }
}
