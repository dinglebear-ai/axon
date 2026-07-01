//! Graph store boundary and in-memory fake.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>>;
    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult>;
    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult>;
    async fn capabilities(&self) -> Result<GraphStoreCapability>;
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

#[async_trait]
impl GraphStore for FakeGraphStore {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult> {
        let mut state = self.state.lock().await;
        let mut nodes_upserted = 0;
        let mut edges_upserted = 0;
        let mut evidence_records = 0;

        for candidate in candidates {
            for node in candidate.nodes {
                let node_id = GraphNodeId::new(node.stable_key.clone());
                state
                    .node_id_by_key
                    .insert(node.stable_key, node_id.clone());
                state.nodes_by_id.insert(
                    node_id.clone(),
                    GraphNode {
                        node_id,
                        kind: node.node_kind,
                        canonical_uri: format!("graph://{}", node.label),
                        display_name: node.label,
                        authority: AuthorityLevel::Inferred,
                        confidence: candidate.confidence,
                        metadata: node.properties,
                        source_ids: vec![candidate.source_id.clone()],
                        created_at: Some(timestamp()),
                        updated_at: Some(timestamp()),
                    },
                );
                nodes_upserted += 1;
            }

            for edge in candidate.edges {
                let Some(from_node_id) = state.node_id_by_key.get(&edge.from_stable_key).cloned()
                else {
                    continue;
                };
                let Some(to_node_id) = state.node_id_by_key.get(&edge.to_stable_key).cloned()
                else {
                    continue;
                };
                let edge_id = GraphEdgeId::new(format!(
                    "{}:{}:{}",
                    edge.edge_kind, from_node_id.0, to_node_id.0
                ));
                evidence_records += candidate.evidence.len() as u64;
                state.edges_by_id.insert(
                    edge_id.clone(),
                    GraphEdge {
                        edge_id,
                        kind: edge.edge_kind,
                        from_node_id,
                        to_node_id,
                        authority: AuthorityLevel::Inferred,
                        confidence: candidate.confidence,
                        evidence: candidate.evidence.clone(),
                        metadata: edge.properties,
                    },
                );
                edges_upserted += 1;
            }
        }

        Ok(GraphWriteResult {
            header: stage_header(),
            source_id: SourceId::new("fake"),
            candidates_seen: nodes_upserted,
            nodes_upserted,
            edges_upserted,
            evidence_records,
            warnings: Vec::new(),
        })
    }

    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>> {
        Ok(self.state.lock().await.nodes_by_id.get(&node_id).cloned())
    }

    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>> {
        Ok(self.state.lock().await.edges_by_id.get(&edge_id).cloned())
    }

    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult> {
        let state = self.state.lock().await;
        let Some(start_node_id) = resolve_identifier(&state, &request.start) else {
            return Ok(GraphQueryResult {
                nodes: Vec::new(),
                edges: Vec::new(),
                evidence: Vec::new(),
                next_cursor: None,
                warnings: Vec::new(),
            });
        };
        let mut nodes = Vec::new();
        if let Some(start) = state.nodes_by_id.get(&start_node_id) {
            nodes.push(start.clone());
        }
        let edges = state
            .edges_by_id
            .values()
            .filter(|edge| edge.from_node_id == start_node_id)
            .filter(|edge| request.edges.is_empty() || request.edges.contains(&edge.kind))
            .cloned()
            .collect::<Vec<_>>();
        for edge in &edges {
            if let Some(node) = state.nodes_by_id.get(&edge.to_node_id) {
                nodes.push(node.clone());
            }
        }
        let evidence = edges
            .iter()
            .flat_map(|edge| edge.evidence.clone())
            .collect::<Vec<_>>();
        Ok(GraphQueryResult {
            nodes,
            edges,
            evidence,
            next_cursor: None,
            warnings: Vec::new(),
        })
    }

    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult> {
        let state = self.state.lock().await;
        let mut resolved = Vec::new();
        let mut misses = Vec::new();
        for identifier in request.identifiers {
            if let Some(node_id) = resolve_identifier(&state, &identifier) {
                if let Some(node) = state.nodes_by_id.get(&node_id).cloned() {
                    let edges = request
                        .include_edges
                        .then(|| {
                            state
                                .edges_by_id
                                .values()
                                .filter(|edge| {
                                    edge.from_node_id == node_id || edge.to_node_id == node_id
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    resolved.push(GraphResolveMatch {
                        identifier,
                        node,
                        confidence: 1.0,
                        evidence: Vec::new(),
                        edges,
                    });
                    continue;
                }
            }
            misses.push(GraphResolveMiss {
                identifier,
                reason: "not found".to_string(),
            });
        }
        Ok(GraphResolveResult {
            resolved,
            misses,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<GraphStoreCapability> {
        Ok(CapabilityBase {
            name: "fake-graph".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-graph".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["candidate_ingest".to_string(), "resolve".to_string()],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

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
