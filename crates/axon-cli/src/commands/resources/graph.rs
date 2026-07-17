use super::{flag_value, parse_u32_flag, positional, print_value};
use axon_api::source::{GraphDirection, GraphIdentifier, GraphQueryRequest, GraphResolveRequest};
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::graph::{self as graph_svc, GraphEdgeId, GraphNodeId, GraphStore, SourceId};
use std::error::Error;

pub(super) async fn run_graph(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        Some("kinds") => print_value(graph_svc::kinds()),
        Some("resolve") => resolve(cfg, context).await,
        Some("query") => query(cfg, context).await,
        Some("node") => node(cfg, context).await,
        Some("edge") => edge(cfg, context).await,
        Some("source") => source(cfg, context).await,
        Some(other) => Err(format!("unknown graph subcommand: {other}").into()),
        None => Err("graph requires kinds|resolve|query|node|edge|source".into()),
    }
}

async fn store(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<graph_svc::SqliteGraphStore, Box<dyn Error>> {
    let pool = context.jobs.sqlite_pool();
    graph_svc::open_graph_store(cfg, pool.as_deref()).await
}

async fn resolve(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let identifier = positional(cfg, 1, "identifier")?;
    let graph = store(cfg, context).await?;
    let result = GraphStore::resolve(
        &graph,
        GraphResolveRequest {
            identifiers: vec![GraphIdentifier {
                kind: flag_value(cfg, "--kind").unwrap_or_default(),
                canonical_uri: identifier.contains("://").then(|| identifier.to_string()),
                value: (!identifier.contains("://")).then(|| identifier.to_string()),
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: Default::default(),
            }],
            include_edges: false,
        },
    )
    .await
    .map_err(api_error)?;
    print_value(result)
}

async fn query(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let graph = store(cfg, context).await?;
    let result = GraphStore::query(
        &graph,
        GraphQueryRequest {
            start: GraphIdentifier {
                kind: String::new(),
                canonical_uri: None,
                value: None,
                node_id: Some(GraphNodeId::new(positional(cfg, 1, "query")?)),
                source_id: None,
                source_item_key: None,
                metadata: Default::default(),
            },
            edges: Vec::new(),
            direction: GraphDirection::Both,
            depth: 1,
            filters: None,
            limit: parse_u32_flag(cfg, "--limit")?.unwrap_or(100),
            cursor: flag_value(cfg, "--cursor"),
        },
    )
    .await
    .map_err(api_error)?;
    print_value(result)
}

async fn node(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let node_id = positional(cfg, 1, "node_id")?;
    let graph = store(cfg, context).await?;
    let detail = graph_svc::node_detail(
        &graph,
        GraphNodeId::new(node_id),
        cfg.positional
            .iter()
            .any(|value| value == "--include-edges"),
    )
    .await
    .map_err(api_error)?
    .ok_or_else(|| format!("graph node {node_id} not found"))?;
    print_value(serde_json::json!({ "node": detail.node, "edges": detail.edges }))
}

async fn edge(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let edge_id = positional(cfg, 1, "edge_id")?;
    let graph = store(cfg, context).await?;
    let edge = GraphStore::get_edge(&graph, GraphEdgeId::new(edge_id))
        .await
        .map_err(api_error)?
        .ok_or_else(|| format!("graph edge {edge_id} not found"))?;
    print_value(edge)
}

async fn source(cfg: &Config, context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let graph = store(cfg, context).await?;
    let result = graph_svc::source_subgraph(
        &graph,
        SourceId::new(positional(cfg, 1, "source_id")?),
        parse_u32_flag(cfg, "--depth")?.unwrap_or(1),
        flag_value(cfg, "--edge-kind"),
        parse_u32_flag(cfg, "--limit")?.unwrap_or(200),
    )
    .await
    .map_err(api_error)?;
    print_value(result)
}

fn api_error(error: axon_api::source::ApiError) -> Box<dyn Error> {
    error.to_string().into()
}
