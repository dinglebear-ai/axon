//! Graph traversal query for the SQLite graph store.

use std::collections::BTreeSet;

use axon_api::source::{
    GraphDirection, GraphEdge, GraphEvidence, GraphNode, GraphNodeId, GraphQueryRequest,
    GraphQueryResult,
};
use sqlx::SqlitePool;

use super::row::{edge_from_row, evidence_from_row, node_from_row};
use crate::error::graph_storage_error;

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

/// Breadth-first traversal from `request.start` up to `request.depth`,
/// following edges in `request.direction`, filtered by `request.edges`
/// (edge-kind allowlist) and `request.limit` (max edges returned).
pub async fn query(pool: &SqlitePool, request: GraphQueryRequest) -> StoreResult<GraphQueryResult> {
    let (max_depth, limit) = crate::store::bounded_query(&request)?;
    let Some(start) = super::resolve::resolve_one(pool, &request.start).await? else {
        return Ok(empty_result());
    };

    let edge_filter: BTreeSet<String> = request.edges.iter().cloned().collect();
    let fetch_limit = limit.saturating_add(1);
    let mut cursor_seen = request.cursor.is_none();

    let mut nodes: Vec<GraphNode> = vec![start.clone()];
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut seen_nodes: BTreeSet<String> = BTreeSet::from([start.node_id.0.clone()]);
    let mut seen_edges: BTreeSet<String> = BTreeSet::new();
    let mut edges_examined = 0usize;
    let mut frontier = vec![start.node_id.clone()];

    for _depth in 0..max_depth {
        if frontier.is_empty() || edges.len() >= fetch_limit || edges_examined >= fetch_limit {
            break;
        }
        let remaining = fetch_limit.saturating_sub(edges_examined);
        let mut incident = Vec::new();
        for node_batch in frontier.chunks(400) {
            let batch_remaining = remaining.saturating_sub(incident.len());
            if batch_remaining == 0 {
                break;
            }
            incident.extend(
                incident_edges(
                    pool,
                    node_batch,
                    request.direction,
                    &edge_filter,
                    batch_remaining,
                )
                .await?,
            );
        }
        incident.sort_by(|left, right| left.edge_id.0.cmp(&right.edge_id.0));
        incident.dedup_by(|left, right| left.edge_id == right.edge_id);
        let frontier_ids = frontier
            .iter()
            .map(|node_id| node_id.0.as_str())
            .collect::<BTreeSet<_>>();
        let mut next_frontier = Vec::new();
        for edge in incident {
            if !edge_filter.is_empty() && !edge_filter.contains(&edge.kind) {
                continue;
            }
            let next = next_node(&edge, &frontier_ids, request.direction);
            if seen_edges.insert(edge.edge_id.0.clone()) {
                edges_examined = edges_examined.saturating_add(1);
                if cursor_seen {
                    edges.push(edge);
                    if edges.len() >= fetch_limit {
                        break;
                    }
                } else if request.cursor.as_deref() == Some(edge.edge_id.0.as_str()) {
                    cursor_seen = true;
                }
            }
            if let Some(next_id) = next
                && seen_nodes.insert(next_id.0.clone())
            {
                next_frontier.push(next_id);
            }
        }
        frontier = next_frontier;
    }

    if !cursor_seen {
        return Err(axon_api::source::ApiError::new(
            "graph.invalid_cursor",
            axon_api::source::ErrorStage::Retrieving,
            "graph query cursor does not identify an edge in this traversal",
        ));
    }
    let has_more = edges.len() > limit;
    edges.truncate(limit);
    nodes.extend(
        load_nodes(
            pool,
            seen_nodes
                .iter()
                .filter(|id| id.as_str() != start.node_id.0.as_str()),
        )
        .await?,
    );
    attach_evidence(pool, &mut edges).await?;
    let next_cursor = has_more.then(|| {
        edges
            .last()
            .expect("a page with a continuation has at least one edge")
            .edge_id
            .0
            .clone()
    });
    let evidence: Vec<GraphEvidence> = edges.iter().flat_map(|e| e.evidence.clone()).collect();
    Ok(GraphQueryResult {
        nodes,
        edges,
        evidence,
        next_cursor,
        warnings: Vec::new(),
    })
}

async fn load_nodes<'a>(
    pool: &SqlitePool,
    node_ids: impl Iterator<Item = &'a String>,
) -> StoreResult<Vec<GraphNode>> {
    let ids = node_ids.collect::<Vec<_>>();
    let mut nodes = Vec::with_capacity(ids.len());
    for batch in ids.chunks(900) {
        let mut builder = sqlx::QueryBuilder::new("SELECT * FROM graph_nodes WHERE node_id IN (");
        let mut separated = builder.separated(", ");
        for node_id in batch {
            separated.push_bind(*node_id);
        }
        separated.push_unseparated(") ORDER BY node_id");
        let rows =
            builder.build().fetch_all(pool).await.map_err(|e| {
                graph_storage_error(format!("failed to fetch graph node batch: {e}"))
            })?;
        nodes.extend(
            rows.iter()
                .map(node_from_row)
                .collect::<StoreResult<Vec<_>>>()?,
        );
    }
    Ok(nodes)
}

/// Fetch edges incident to `node_id` in the requested direction.
async fn incident_edges(
    pool: &SqlitePool,
    node_ids: &[GraphNodeId],
    direction: GraphDirection,
    edge_filter: &BTreeSet<String>,
    limit: usize,
) -> StoreResult<Vec<GraphEdge>> {
    let mut builder = sqlx::QueryBuilder::new("SELECT * FROM graph_edges WHERE ");
    match direction {
        GraphDirection::Out => {
            builder.push("from_node_id IN (");
            push_node_ids(&mut builder, node_ids);
            builder.push(")");
        }
        GraphDirection::In => {
            builder.push("to_node_id IN (");
            push_node_ids(&mut builder, node_ids);
            builder.push(")");
        }
        GraphDirection::Both => {
            builder.push("(from_node_id IN (");
            push_node_ids(&mut builder, node_ids);
            builder.push(") OR to_node_id IN (");
            push_node_ids(&mut builder, node_ids);
            builder.push("))");
        }
    }
    if !edge_filter.is_empty() {
        builder.push(" AND kind IN (");
        let mut separated = builder.separated(", ");
        for kind in edge_filter {
            separated.push_bind(kind);
        }
        separated.push_unseparated(")");
    }
    builder
        .push(" ORDER BY edge_id LIMIT ")
        .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
    let rows = builder
        .build()
        .fetch_all(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to fetch incident edges: {e}")))?;
    rows.iter().map(edge_from_row).collect()
}

fn push_node_ids<'a>(
    builder: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    node_ids: &'a [GraphNodeId],
) {
    let mut separated = builder.separated(", ");
    for node_id in node_ids {
        separated.push_bind(&node_id.0);
    }
}

pub(super) async fn attach_evidence(pool: &SqlitePool, edges: &mut [GraphEdge]) -> StoreResult<()> {
    if edges.is_empty() {
        return Ok(());
    }
    let mut by_edge = std::collections::BTreeMap::<String, Vec<GraphEvidence>>::new();
    for edge_batch in edges.chunks(900) {
        let mut builder =
            sqlx::QueryBuilder::new("SELECT * FROM graph_evidence WHERE edge_id IN (");
        let mut separated = builder.separated(", ");
        for edge in edge_batch {
            separated.push_bind(&edge.edge_id.0);
        }
        separated.push_unseparated(") ORDER BY edge_id, evidence_id");
        let rows = builder.build().fetch_all(pool).await.map_err(|e| {
            graph_storage_error(format!("failed to fetch graph evidence batch: {e}"))
        })?;
        for row in &rows {
            let evidence = evidence_from_row(row)?;
            let edge_id: String = sqlx::Row::try_get(row, "edge_id").map_err(|e| {
                graph_storage_error(format!("failed to decode evidence edge id: {e}"))
            })?;
            by_edge.entry(edge_id).or_default().push(evidence);
        }
    }
    for edge in edges {
        edge.evidence = by_edge.remove(&edge.edge_id.0).unwrap_or_default();
    }
    Ok(())
}

/// The node on the far side of `edge` from `node_id`, per direction.
fn next_node(
    edge: &GraphEdge,
    frontier: &BTreeSet<&str>,
    direction: GraphDirection,
) -> Option<GraphNodeId> {
    match direction {
        GraphDirection::Out if frontier.contains(edge.from_node_id.0.as_str()) => {
            Some(edge.to_node_id.clone())
        }
        GraphDirection::In if frontier.contains(edge.to_node_id.0.as_str()) => {
            Some(edge.from_node_id.clone())
        }
        GraphDirection::Both if frontier.contains(edge.from_node_id.0.as_str()) => {
            Some(edge.to_node_id.clone())
        }
        GraphDirection::Both if frontier.contains(edge.to_node_id.0.as_str()) => {
            Some(edge.from_node_id.clone())
        }
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
