//! Graph store boundary and in-memory fake.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
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

    /// Delete graph nodes identified by stable key, cascading to their
    /// incident edges (and, for the durable store, evidence/aliases via FK
    /// `ON DELETE CASCADE`). Used by cleanup-debt `GraphPrune` drains — per
    /// the pruning contract, graph orphan cleanup is identity-scoped, never a
    /// blanket `reset()`. Idempotent: deleting an unknown stable key is a
    /// no-op.
    async fn delete_nodes(&self, stable_keys: Vec<String>) -> Result<GraphDeleteResult>;

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

#[async_trait]
impl GraphStore for FakeGraphStore {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult> {
        let mut state = self.state.lock().await;
        let candidates_seen = candidates.len() as u64;
        let source_id = candidates
            .first()
            .map(|candidate| candidate.source_id.clone())
            .unwrap_or_else(|| SourceId::new("fake"));
        let mut nodes_upserted = 0;
        let mut edges_upserted = 0;
        let mut evidence_records = 0;
        let mut warnings = Vec::new();

        for candidate in candidates {
            for node in candidate.nodes {
                let node_id = GraphNodeId::new(node.stable_key.clone());
                if let Some(existing) = state.nodes_by_id.get(&node_id)
                    && existing.kind != node.node_kind
                {
                    warnings.push(SourceWarning {
                        code: "graph.node_kind_conflict".to_string(),
                        message: format!(
                            "graph node {} was previously kind {} but candidate {} reported {}",
                            node_id.0, existing.kind, candidate.candidate_id, node.node_kind
                        ),
                        severity: Severity::Warning,
                        source_item_key: Some(candidate.source_item_key.clone()),
                        retryable: false,
                    });
                    continue;
                }
                state
                    .node_id_by_key
                    .insert(node.stable_key, node_id.clone());
                if let Some(existing) = state.nodes_by_id.get_mut(&node_id) {
                    existing.confidence = existing.confidence.max(candidate.confidence);
                    if !existing.source_ids.contains(&candidate.source_id) {
                        existing.source_ids.push(candidate.source_id.clone());
                    }
                    existing.updated_at = Some(timestamp());
                } else {
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
                let edge_evidence = candidate
                    .evidence
                    .iter()
                    .filter(|evidence| edge.evidence_ids.contains(&evidence.evidence_id))
                    .cloned()
                    .collect();
                let new_edge = GraphEdge {
                    edge_id: edge_id.clone(),
                    kind: edge.edge_kind,
                    from_node_id,
                    to_node_id,
                    authority: AuthorityLevel::Inferred,
                    confidence: candidate.confidence,
                    evidence: edge_evidence,
                    metadata: edge.properties,
                };

                if let Some(existing) = state.edges_by_id.get_mut(&edge_id) {
                    for evidence in new_edge.evidence {
                        if !existing
                            .evidence
                            .iter()
                            .any(|stored| stored.evidence_id == evidence.evidence_id)
                        {
                            existing.evidence.push(evidence);
                            evidence_records += 1;
                        }
                    }
                    existing.confidence = existing.confidence.max(new_edge.confidence);
                } else {
                    evidence_records += new_edge.evidence.len() as u64;
                    state.edges_by_id.insert(edge_id, new_edge);
                    edges_upserted += 1;
                }
            }
        }

        Ok(GraphWriteResult {
            header: stage_header(),
            source_id,
            candidates_seen,
            nodes_upserted,
            edges_upserted,
            evidence_records,
            warnings,
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
        let mut seen_nodes = BTreeSet::from([start_node_id.clone()]);
        let mut seen_edges = BTreeSet::new();
        let mut frontier = VecDeque::from([(start_node_id, 0)]);
        let mut edges = Vec::new();
        let max_depth = request.depth;
        let limit = usize::try_from(request.limit)
            .ok()
            .filter(|limit| *limit > 0)
            .unwrap_or(usize::MAX);

        while let Some((node_id, depth)) = frontier.pop_front() {
            if depth >= max_depth || edges.len() >= limit {
                continue;
            }

            for edge in state.edges_by_id.values() {
                if !request.edges.is_empty() && !request.edges.contains(&edge.kind) {
                    continue;
                }
                let Some(next_node_id) = edge_next_node(edge, &node_id, request.direction) else {
                    continue;
                };
                if seen_edges.insert(edge.edge_id.clone()) {
                    edges.push(edge.clone());
                    if edges.len() >= limit {
                        break;
                    }
                }
                if seen_nodes.insert(next_node_id.clone()) {
                    if let Some(node) = state.nodes_by_id.get(&next_node_id) {
                        nodes.push(node.clone());
                    }
                    frontier.push_back((next_node_id, depth + 1));
                }
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
            if let Some(node_id) = resolve_identifier(&state, &identifier)
                && let Some(node) = state.nodes_by_id.get(&node_id).cloned()
            {
                let edges = if request.include_edges {
                    state
                        .edges_by_id
                        .values()
                        .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
                        .cloned()
                        .collect()
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
                continue;
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

    async fn node_edges(&self, node_id: GraphNodeId) -> Result<Vec<GraphEdge>> {
        let state = self.state.lock().await;
        Ok(state
            .edges_by_id
            .values()
            .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
            .cloned()
            .collect())
    }

    async fn nodes_for_source(&self, source_id: SourceId) -> Result<Vec<GraphNode>> {
        let state = self.state.lock().await;
        Ok(state
            .nodes_by_id
            .values()
            .filter(|node| node.source_ids.contains(&source_id))
            .cloned()
            .collect())
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

    async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeGraphState::default();
        Ok(())
    }

    async fn delete_nodes(&self, stable_keys: Vec<String>) -> Result<GraphDeleteResult> {
        let mut state = self.state.lock().await;
        let mut nodes_deleted = 0;
        let mut edges_deleted = 0;
        for stable_key in stable_keys {
            let Some(node_id) = state.node_id_by_key.remove(&stable_key) else {
                continue;
            };
            if state.nodes_by_id.remove(&node_id).is_some() {
                nodes_deleted += 1;
            }
            let incident: Vec<GraphEdgeId> = state
                .edges_by_id
                .values()
                .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
                .map(|edge| edge.edge_id.clone())
                .collect();
            for edge_id in incident {
                if state.edges_by_id.remove(&edge_id).is_some() {
                    edges_deleted += 1;
                }
            }
        }
        Ok(GraphDeleteResult {
            nodes_deleted,
            edges_deleted,
        })
    }

    async fn delete_edges(&self, edge_ids: Vec<GraphEdgeId>) -> Result<GraphDeleteResult> {
        let mut state = self.state.lock().await;
        let mut edges_deleted = 0;
        for edge_id in edge_ids {
            if state.edges_by_id.remove(&edge_id).is_some() {
                edges_deleted += 1;
            }
        }
        Ok(GraphDeleteResult {
            nodes_deleted: 0,
            edges_deleted,
        })
    }
}

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
