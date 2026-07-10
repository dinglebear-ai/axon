//! `GraphService` — entity graph read surface (kinds/resolve/query/get_node/
//! get_edge).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §GraphService. `axon-services` has no `graph.rs` module and no free
//! functions that read/query `axon-graph` today — only `source/graph.rs`
//! *writes* graph candidates during ingest. All five methods are therefore
//! FAKE_ONLY: implementing real graph query orchestration is out of this
//! workstream's scope (see the module's non-goals). Only the `Fake`
//! implements real (in-memory) semantics.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    GraphEdge, GraphEdgeId, GraphKindDocument, GraphNode, GraphNodeId, GraphQueryRequest,
    GraphQueryResult, GraphResolveRequest, GraphResolveResult,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

#[async_trait]
pub trait GraphService: Send + Sync {
    async fn kinds(&self) -> anyhow::Result<GraphKindDocument>;
    async fn resolve(&self, request: GraphResolveRequest) -> anyhow::Result<GraphResolveResult>;
    async fn query(&self, request: GraphQueryRequest) -> anyhow::Result<GraphQueryResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> anyhow::Result<GraphNode>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> anyhow::Result<GraphEdge>;
}

pub struct GraphServiceImpl {
    #[allow(dead_code)]
    ctx: Arc<ServiceContext>,
}

impl GraphServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl GraphService for GraphServiceImpl {
    async fn kinds(&self) -> anyhow::Result<GraphKindDocument> {
        Err(not_implemented("GraphService::kinds"))
    }

    async fn resolve(&self, _request: GraphResolveRequest) -> anyhow::Result<GraphResolveResult> {
        Err(not_implemented("GraphService::resolve"))
    }

    async fn query(&self, _request: GraphQueryRequest) -> anyhow::Result<GraphQueryResult> {
        Err(not_implemented("GraphService::query"))
    }

    async fn get_node(&self, _node_id: GraphNodeId) -> anyhow::Result<GraphNode> {
        Err(not_implemented("GraphService::get_node"))
    }

    async fn get_edge(&self, _edge_id: GraphEdgeId) -> anyhow::Result<GraphEdge> {
        Err(not_implemented("GraphService::get_edge"))
    }
}

/// Deterministic in-memory fake covering every `GraphService` method.
#[derive(Default)]
pub struct FakeGraphService {
    nodes: Mutex<std::collections::HashMap<String, GraphNode>>,
    edges: Mutex<std::collections::HashMap<String, GraphEdge>>,
}

impl FakeGraphService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_node(&self, node: GraphNode) {
        self.nodes
            .lock()
            .unwrap()
            .insert(node.node_id.0.clone(), node);
    }

    pub fn seed_edge(&self, edge: GraphEdge) {
        self.edges
            .lock()
            .unwrap()
            .insert(edge.edge_id.0.clone(), edge);
    }
}

#[async_trait]
impl GraphService for FakeGraphService {
    async fn kinds(&self) -> anyhow::Result<GraphKindDocument> {
        Ok(GraphKindDocument {
            node_kinds: vec!["entity".to_string()],
            edge_kinds: vec!["relates_to".to_string()],
            evidence_kinds: vec!["source_extraction".to_string()],
            authority_levels: vec![axon_api::source::AuthorityLevel::Inferred],
        })
    }

    async fn resolve(&self, request: GraphResolveRequest) -> anyhow::Result<GraphResolveResult> {
        let nodes = self.nodes.lock().unwrap();
        let mut resolved = Vec::new();
        let mut misses = Vec::new();
        for identifier in request.identifiers {
            let matched = identifier
                .node_id
                .as_ref()
                .and_then(|id| nodes.get(&id.0).cloned());
            match matched {
                Some(node) => resolved.push(axon_api::source::GraphResolveMatch {
                    identifier,
                    node,
                    confidence: 1.0,
                    evidence: Vec::new(),
                    edges: Vec::new(),
                }),
                None => misses.push(axon_api::source::GraphResolveMiss {
                    identifier,
                    reason: "no matching node in fake graph store".to_string(),
                }),
            }
        }
        Ok(GraphResolveResult {
            resolved,
            misses,
            warnings: Vec::new(),
        })
    }

    async fn query(&self, request: GraphQueryRequest) -> anyhow::Result<GraphQueryResult> {
        let nodes = self.nodes.lock().unwrap();
        let items: Vec<GraphNode> = nodes
            .values()
            .take(request.limit as usize)
            .cloned()
            .collect();
        Ok(GraphQueryResult {
            nodes: items,
            edges: Vec::new(),
            evidence: Vec::new(),
            next_cursor: None,
            warnings: Vec::new(),
        })
    }

    async fn get_node(&self, node_id: GraphNodeId) -> anyhow::Result<GraphNode> {
        self.nodes
            .lock()
            .unwrap()
            .get(&node_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("graph node {} not found", node_id.0))
    }

    async fn get_edge(&self, edge_id: GraphEdgeId) -> anyhow::Result<GraphEdge> {
        self.edges
            .lock()
            .unwrap()
            .get(&edge_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("graph edge {} not found", edge_id.0))
    }
}

#[cfg(test)]
#[path = "graph_service_tests.rs"]
mod tests;
