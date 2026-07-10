//! Read-only SourceGraph facade (issue #298 GQ tail).
//!
//! Thin wrapper over [`axon_graph::store::GraphStore`] /
//! [`axon_graph::sqlite::SqliteGraphStore`] for REST (`/v1/graph/*`) and MCP
//! (`action=graph`) handlers. Contract:
//! `docs/pipeline-unification/sources/source-graph.md` +
//! `docs/pipeline-unification/surfaces/{rest-contract,tool-contract}.md`
//! Graph rows.
//!
//! `GraphStore` writes stay parser/source-job owned
//! (`crate::source::graph::write_baseline_graph`); this module only reads.

use std::collections::BTreeSet;
use std::error::Error;

use axon_core::config::Config;
use sqlx::SqlitePool;

pub use axon_api::source::{
    ApiError, GraphDirection, GraphEdge, GraphEdgeId, GraphIdentifier, GraphKindDocument,
    GraphNode, GraphNodeId, GraphQueryRequest, GraphQueryResult, GraphResolveRequest,
    GraphResolveResult, SourceId,
};
pub use axon_graph::sqlite::SqliteGraphStore;
pub use axon_graph::store::GraphStore;

/// Open a durable graph store against the given shared pool, or against a
/// freshly opened config-derived pool when no shared pool is available
/// (mirrors `watch::open_source_watch_store`'s fallback pattern). The graph
/// tables are created idempotently either way — by the composed unified
/// migration runner when a shared pool is passed, or by
/// [`axon_graph::migration::ensure_schema`] via [`SqliteGraphStore::connect`]-
/// equivalent behavior otherwise.
pub async fn open_graph_store(
    cfg: &Config,
    pool: Option<&SqlitePool>,
) -> Result<SqliteGraphStore, Box<dyn Error>> {
    let pool = match pool {
        Some(pool) => pool.clone(),
        None => axon_jobs::store::open_config_pool(cfg).await?,
    };
    axon_graph::migration::ensure_schema(&pool)
        .await
        .map_err(|err| -> Box<dyn Error> { Box::new(GraphFacadeError(err.message)) })?;
    Ok(SqliteGraphStore::from_pool(pool))
}

#[derive(Debug)]
struct GraphFacadeError(String);

impl std::fmt::Display for GraphFacadeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for GraphFacadeError {}

/// Supported node/edge/evidence kinds + authority levels (`graph.kinds` /
/// `GET /v1/graph/kinds`). Pure — derived from `axon-graph`'s closed
/// registries, no store access required.
pub fn kinds() -> GraphKindDocument {
    axon_graph::schema_registry::kind_document()
}

/// A node plus its incident edges — the shape backing `GET
/// /v1/graph/nodes/{node_id}` (bare node) and `GET
/// /v1/graph/nodes/{node_id}/edges` (edges only, node included for context).
#[derive(Debug, Clone)]
pub struct GraphNodeDetail {
    pub node: GraphNode,
    pub edges: Vec<GraphEdge>,
}

/// Fetch a node by id, optionally with its incident edges loaded.
pub async fn node_detail(
    store: &dyn GraphStore,
    node_id: GraphNodeId,
    include_edges: bool,
) -> Result<Option<GraphNodeDetail>, ApiError> {
    let Some(node) = store.get_node(node_id.clone()).await? else {
        return Ok(None);
    };
    let edges = if include_edges {
        store.node_edges(node_id).await?
    } else {
        Vec::new()
    };
    Ok(Some(GraphNodeDetail { node, edges }))
}

/// Source-linked subgraph (`GET /v1/graph/sources/{source_id}` / MCP
/// `graph.source`): every node directly tied to `source_id` via
/// `GraphNode::source_ids`, plus edges/nodes reachable from them within
/// `depth` hops (both directions), optionally filtered to one edge kind.
///
/// `depth == 0` returns only the directly-tied nodes with no edge expansion.
/// `limit` bounds the total number of edges returned across all expansions.
pub async fn source_subgraph(
    store: &dyn GraphStore,
    source_id: SourceId,
    depth: u32,
    edge_kind: Option<String>,
    limit: u32,
) -> Result<GraphQueryResult, ApiError> {
    let direct_nodes = store.nodes_for_source(source_id).await?;
    let mut seen_nodes: BTreeSet<String> = BTreeSet::new();
    let mut nodes = Vec::new();
    for direct in &direct_nodes {
        if seen_nodes.insert(direct.node_id.0.clone()) {
            nodes.push(direct.clone());
        }
    }

    if direct_nodes.is_empty() || depth == 0 {
        return Ok(GraphQueryResult {
            nodes,
            edges: Vec::new(),
            evidence: Vec::new(),
            next_cursor: None,
            warnings: Vec::new(),
        });
    }

    let mut seen_edges: BTreeSet<String> = BTreeSet::new();
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    let edge_filter: Vec<String> = edge_kind.into_iter().collect();

    for direct in direct_nodes {
        if edges.len() as u32 >= limit {
            break;
        }
        let remaining = limit.saturating_sub(edges.len() as u32);
        let expanded = store
            .query(GraphQueryRequest {
                start: GraphIdentifier {
                    kind: direct.kind.clone(),
                    canonical_uri: None,
                    value: None,
                    node_id: Some(direct.node_id.clone()),
                    source_id: None,
                    source_item_key: None,
                    metadata: Default::default(),
                },
                edges: edge_filter.clone(),
                direction: GraphDirection::Both,
                depth,
                filters: None,
                limit: remaining,
                cursor: None,
            })
            .await?;
        for node in expanded.nodes {
            if seen_nodes.insert(node.node_id.0.clone()) {
                nodes.push(node);
            }
        }
        for edge in expanded.edges {
            if seen_edges.insert(edge.edge_id.0.clone()) {
                edges.push(edge);
            }
        }
        evidence.extend(expanded.evidence);
    }

    Ok(GraphQueryResult {
        nodes,
        edges,
        evidence,
        next_cursor: None,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
#[path = "graph_service_tests.rs"]
mod tests;
