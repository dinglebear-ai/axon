//! `action=graph` — the read-only SourceGraph query surface (issue #298 GQ),
//! mirroring the REST `/v1/graph/*` routes
//! (`docs/pipeline-unification/surfaces/rest-contract.md` "Graph Routes",
//! `docs/pipeline-unification/surfaces/tool-contract.md` "Graph subactions").
//!
//! Every subaction here is a pure read. Graph writes stay parser/source-job
//! owned (`axon_services::source::graph::write_baseline_graph`) — this
//! action never accepts caller-provided nodes/edges.

use super::AxonMcpServer;
use super::artifacts::{InlineHint, respond_with_mode};
use super::common::{invalid_params, logged_internal_error};
use crate::schema::{AxonToolResponse, GraphDirectionArg, GraphRequest, GraphSubaction};
use axon_api::source::{GraphDirection, GraphIdentifier, GraphQueryRequest, GraphResolveRequest};
use axon_services::graph::{self as graph_svc, GraphEdgeId, GraphNodeId, GraphStore, SourceId};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_graph(
        &self,
        req: GraphRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or(GraphSubaction::Kinds);
        match subaction {
            GraphSubaction::Kinds => self.graph_kinds(req).await,
            GraphSubaction::Resolve => self.graph_resolve(req).await,
            GraphSubaction::Query => self.graph_query(req).await,
            GraphSubaction::Node => self.graph_node(req).await,
            GraphSubaction::Edge => self.graph_edge(req).await,
            GraphSubaction::Source => self.graph_source(req).await,
        }
    }

    async fn graph_open_store(&self) -> Result<graph_svc::SqliteGraphStore, ErrorData> {
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("graph.context", e.as_ref()))?;
        let pool = ctx.jobs.sqlite_pool();
        graph_svc::open_graph_store(self.cfg.as_ref(), pool.as_deref())
            .await
            .map_err(|e| logged_internal_error("graph.open_store", e.as_ref()))
    }

    async fn graph_kinds(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        respond_with_mode(
            "graph",
            "kinds",
            req.response_mode,
            "graph-kinds",
            serde_json::json!(graph_svc::kinds()),
            InlineHint::Default,
        )
        .await
    }

    fn graph_require_id(req: &GraphRequest) -> Result<String, ErrorData> {
        req.id
            .clone()
            .ok_or_else(|| invalid_params("graph subaction requires 'id'"))
    }

    async fn graph_resolve(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        let store = self.graph_open_store().await?;
        let identifier = GraphIdentifier {
            kind: req.kind.clone().unwrap_or_default(),
            canonical_uri: req.canonical_uri.clone(),
            value: req
                .canonical_uri
                .is_none()
                .then(|| req.id.clone())
                .flatten(),
            node_id: None,
            source_id: None,
            source_item_key: None,
            metadata: Default::default(),
        };
        let result = GraphStore::resolve(
            &store,
            GraphResolveRequest {
                identifiers: vec![identifier],
                include_edges: req.include_edges.unwrap_or(false),
            },
        )
        .await
        .map_err(|e| invalid_params(e.message))?;
        respond_with_mode(
            "graph",
            "resolve",
            req.response_mode,
            "graph-resolve",
            serde_json::json!(result),
            InlineHint::Default,
        )
        .await
    }

    async fn graph_query(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        let store = self.graph_open_store().await?;
        let start_id = req
            .node_id
            .clone()
            .or_else(|| req.id.clone())
            .ok_or_else(|| invalid_params("graph query requires 'node_id' (or 'id')"))?;
        let result = GraphStore::query(
            &store,
            GraphQueryRequest {
                start: GraphIdentifier {
                    kind: req.kind.clone().unwrap_or_default(),
                    canonical_uri: None,
                    value: None,
                    node_id: Some(GraphNodeId::new(start_id)),
                    source_id: None,
                    source_item_key: None,
                    metadata: Default::default(),
                },
                edges: req.edges.clone().unwrap_or_default(),
                direction: to_direction(req.direction),
                depth: req.depth.unwrap_or(1),
                filters: None,
                limit: req.limit.unwrap_or(100),
                cursor: req.cursor.clone(),
            },
        )
        .await
        .map_err(|e| invalid_params(e.message))?;
        respond_with_mode(
            "graph",
            "query",
            req.response_mode,
            "graph-query",
            serde_json::json!(result),
            InlineHint::Default,
        )
        .await
    }

    async fn graph_node(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        let node_id = Self::graph_require_id(&req)?;
        let store = self.graph_open_store().await?;
        let detail = graph_svc::node_detail(
            &store,
            GraphNodeId::new(node_id.clone()),
            req.include_edges.unwrap_or(false),
        )
        .await
        .map_err(|e| invalid_params(e.message))?
        .ok_or_else(|| invalid_params(format!("graph node {node_id} not found")))?;
        respond_with_mode(
            "graph",
            "node",
            req.response_mode,
            "graph-node",
            serde_json::json!({ "node": detail.node, "edges": detail.edges }),
            InlineHint::Default,
        )
        .await
    }

    async fn graph_edge(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        let edge_id = Self::graph_require_id(&req)?;
        let store = self.graph_open_store().await?;
        let edge = GraphStore::get_edge(&store, GraphEdgeId::new(edge_id.clone()))
            .await
            .map_err(|e| invalid_params(e.message))?
            .ok_or_else(|| invalid_params(format!("graph edge {edge_id} not found")))?;
        respond_with_mode(
            "graph",
            "edge",
            req.response_mode,
            "graph-edge",
            serde_json::json!(edge),
            InlineHint::Default,
        )
        .await
    }

    async fn graph_source(&self, req: GraphRequest) -> Result<AxonToolResponse, ErrorData> {
        let source_id = Self::graph_require_id(&req)?;
        let store = self.graph_open_store().await?;
        let result = graph_svc::source_subgraph(
            &store,
            SourceId::new(source_id),
            req.depth.unwrap_or(1),
            req.edge_kind.clone(),
            req.limit.unwrap_or(200),
        )
        .await
        .map_err(|e| invalid_params(e.message))?;
        respond_with_mode(
            "graph",
            "source",
            req.response_mode,
            "graph-source",
            serde_json::json!(result),
            InlineHint::Default,
        )
        .await
    }
}

fn to_direction(direction: Option<GraphDirectionArg>) -> GraphDirection {
    match direction {
        Some(GraphDirectionArg::In) => GraphDirection::In,
        Some(GraphDirectionArg::Out) => GraphDirection::Out,
        Some(GraphDirectionArg::Both) | None => GraphDirection::Both,
    }
}
