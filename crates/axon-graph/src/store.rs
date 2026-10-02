//! Graph store boundary and in-memory fake.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

pub type Result<T> = std::result::Result<T, ApiError>;
mod limits;
mod retirement;
pub use limits::{
    DEFAULT_GRAPH_EDGE_LIMIT, MAX_GRAPH_DEPTH, MAX_GRAPH_EDGE_KINDS, MAX_GRAPH_EDGE_LIMIT,
    MAX_GRAPH_IDENTIFIER_BYTES, bounded_limits, bounded_query,
};

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>>;
    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult>;
    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<GraphStoreCapability>;

    /// All edges incident to `node_id` (both directions), with evidence
    /// loaded. Used by node-detail reads (REST `GET /v1/graph/nodes/{id}/edges`,
    /// MCP `graph.node`).
    async fn node_edges(&self, node_id: GraphNodeId) -> Result<Vec<GraphEdge>>;

    /// All nodes whose `source_ids` contains `source_id`. Used by the
    /// source-linked subgraph read (REST `GET /v1/graph/sources/{source_id}`,
    /// MCP `graph.source`).
    async fn nodes_for_source(&self, source_id: SourceId) -> Result<Vec<GraphNode>>;

    async fn nodes_for_source_limited(
        &self,
        source_id: SourceId,
        limit: usize,
    ) -> Result<Vec<GraphNode>> {
        let mut nodes = self.nodes_for_source(source_id).await?;
        nodes.truncate(limit);
        Ok(nodes)
    }

    /// Delete graph nodes identified by stable key, cascading to their
    /// incident edges (and, for the durable store, evidence/aliases via FK
    /// `ON DELETE CASCADE`). Used by cleanup-debt `GraphPrune` drains — per
    /// the pruning contract, graph orphan cleanup is identity-scoped, never a
    /// blanket `reset()`. Idempotent: deleting an unknown stable key is a
    /// no-op.
    async fn delete_nodes(&self, stable_keys: Vec<String>) -> Result<GraphDeleteResult>;

    /// Retire item evidence and unsupported incident output. The caller must
    /// hold the source publication lease and verify the current item disposition.
    async fn retire_item_evidence(
        &self,
        source_id: SourceId,
        item: SourceItemKey,
    ) -> Result<GraphDeleteResult>;

    /// Retire a bounded group under one caller-owned source lease.
    async fn retire_items_evidence(
        &self,
        source_id: SourceId,
        items: Vec<SourceItemKey>,
    ) -> Result<GraphDeleteResult> {
        retirement::retire_many(self, source_id, items).await
    }

    /// Delete graph edges by id. Idempotent: deleting an unknown edge id is a
    /// no-op.
    async fn delete_edges(&self, edge_ids: Vec<GraphEdgeId>) -> Result<GraphDeleteResult>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeGraphStore {
    state: Arc<Mutex<FakeGraphState>>,
}

#[derive(Debug, Default)]
struct FakeGraphState {
    nodes_by_id: BTreeMap<GraphNodeId, GraphNode>,
    node_id_by_key: BTreeMap<String, GraphNodeId>,
    edges_by_id: BTreeMap<GraphEdgeId, GraphEdge>,
}

impl FakeGraphStore {
    pub fn new() -> Self {
        Self::default()
    }
}

mod fake;

fn edge_next_node(
    edge: &GraphEdge,
    node_id: &GraphNodeId,
    direction: GraphDirection,
) -> Option<GraphNodeId> {
    match direction {
        GraphDirection::In if edge.to_node_id == *node_id => Some(edge.from_node_id.clone()),
        GraphDirection::Out if edge.from_node_id == *node_id => Some(edge.to_node_id.clone()),
        GraphDirection::Both if edge.from_node_id == *node_id => Some(edge.to_node_id.clone()),
        GraphDirection::Both if edge.to_node_id == *node_id => Some(edge.from_node_id.clone()),
        _ => None,
    }
}

/// Resolve identifiers in the fake's limited index.
///
/// This fake intentionally supports only direct `node_id` lookup and
/// value-based lookup through `node_id_by_key`, where the value is the node
/// candidate's stable key. It does not resolve `canonical_uri`, `kind`,
/// `source_id`, or `source_item_key` addressing.
fn resolve_identifier(state: &FakeGraphState, identifier: &GraphIdentifier) -> Option<GraphNodeId> {
    identifier.node_id.clone().or_else(|| {
        identifier
            .value
            .as_ref()
            .and_then(|value| state.node_id_by_key.get(value).cloned())
    })
}

fn stage_header() -> StageResultHeader {
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase: PipelinePhase::Graphing,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}
