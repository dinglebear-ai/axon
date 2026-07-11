//! `/v1/graph/*` — the read-only SourceGraph surface (issue #298 GQ,
//! `docs/pipeline-unification/surfaces/rest-contract.md` "Graph Routes").
//!
//! Graph routes expose `SourceGraph`; they do not write arbitrary
//! caller-provided edges. Normal graph writes come from trusted source jobs
//! and parser outputs (`axon_services::source::graph::write_baseline_graph`).
//! All routes here are `axon:read`.

use axon_api::source::{GraphQueryRequest, GraphResolveRequest};
use axon_core::config::Config;
use axon_services::graph as graph_svc;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, std::sync::Arc<Config>);

async fn open_store(
    state: &super::super::state::AppState,
    cfg: &Config,
) -> Result<graph_svc::SqliteGraphStore, HttpError> {
    let pool = state.service_context.jobs.sqlite_pool();
    graph_svc::open_graph_store(cfg, pool.as_deref())
        .await
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/graph/kinds",
    responses(
        (status = 200, description = "Supported node/edge/evidence kinds and authority levels", body = axon_api::source::GraphKindDocument),
    ),
    tag = "graph"
)]
pub(crate) async fn kinds() -> Json<axon_api::source::GraphKindDocument> {
    Json(graph_svc::kinds())
}

#[utoipa::path(
    post,
    path = "/v1/graph/resolve",
    request_body = GraphResolveRequest,
    responses(
        (status = 200, description = "Resolved graph nodes for each identifier", body = serde_json::Value),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn resolve(
    State((state, cfg)): State<WebState>,
    Json(request): Json<GraphResolveRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let result = graph_svc::GraphStore::resolve(&store, request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(result)))
}

#[utoipa::path(
    post,
    path = "/v1/graph/query",
    operation_id = "graph_query",
    request_body = GraphQueryRequest,
    responses(
        (status = 200, description = "Typed graph traversal result", body = serde_json::Value),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn query(
    State((state, cfg)): State<WebState>,
    Json(request): Json<GraphQueryRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let result = graph_svc::GraphStore::query(&store, request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(result)))
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub(crate) struct NodeDetailQuery {
    #[serde(default)]
    include_edges: bool,
}

#[utoipa::path(
    get,
    path = "/v1/graph/nodes/{node_id}",
    params(("node_id" = String, Path, description = "Graph node id"), NodeDetailQuery),
    responses(
        (status = 200, description = "Node detail", body = serde_json::Value),
        (status = 404, description = "Node not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn get_node(
    State((state, cfg)): State<WebState>,
    Path(node_id): Path<String>,
    Query(query): Query<NodeDetailQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let detail = graph_svc::node_detail(
        &store,
        graph_svc::GraphNodeId::new(node_id.clone()),
        query.include_edges,
    )
    .await
    .map_err(HttpError::from_api_error)?;
    match detail {
        Some(detail) => Ok(Json(json!({
            "node": detail.node,
            "edges": detail.edges,
        }))),
        None => Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("graph node {node_id} not found"),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/graph/nodes/{node_id}/edges",
    params(("node_id" = String, Path, description = "Graph node id")),
    responses(
        (status = 200, description = "Edges incident to the node", body = serde_json::Value),
        (status = 404, description = "Node not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn get_node_edges(
    State((state, cfg)): State<WebState>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let detail = graph_svc::node_detail(&store, graph_svc::GraphNodeId::new(node_id.clone()), true)
        .await
        .map_err(HttpError::from_api_error)?;
    match detail {
        Some(detail) => Ok(Json(json!({ "edges": detail.edges }))),
        None => Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("graph node {node_id} not found"),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/graph/edges/{edge_id}",
    params(("edge_id" = String, Path, description = "Graph edge id")),
    responses(
        (status = 200, description = "Edge detail and evidence", body = serde_json::Value),
        (status = 404, description = "Edge not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn get_edge(
    State((state, cfg)): State<WebState>,
    Path(edge_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let edge =
        graph_svc::GraphStore::get_edge(&store, graph_svc::GraphEdgeId::new(edge_id.clone()))
            .await
            .map_err(HttpError::from_api_error)?;
    match edge {
        Some(edge) => Ok(Json(json!(edge))),
        None => Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("graph edge {edge_id} not found"),
        )),
    }
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub(crate) struct SourceSubgraphQuery {
    depth: Option<u32>,
    edge_kind: Option<String>,
    limit: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/v1/graph/sources/{source_id}",
    params(("source_id" = String, Path, description = "Ledger source id"), SourceSubgraphQuery),
    responses(
        (status = 200, description = "Graph nodes/edges tied to the source", body = serde_json::Value),
        (status = 502, description = "Graph storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "graph"
)]
pub(crate) async fn get_source_subgraph(
    State((state, cfg)): State<WebState>,
    Path(source_id): Path<String>,
    Query(query): Query<SourceSubgraphQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let result = graph_svc::source_subgraph(
        &store,
        graph_svc::SourceId::new(source_id),
        query.depth.unwrap_or(1),
        query.edge_kind,
        query.limit.unwrap_or(200),
    )
    .await
    .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(result)))
}
