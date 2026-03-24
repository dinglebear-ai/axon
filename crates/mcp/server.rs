#[path = "server/artifacts.rs"]
pub(super) mod artifacts;
#[path = "server/common.rs"]
pub mod common;
#[path = "server/handlers_acp.rs"]
mod handlers_acp;
#[path = "server/handlers_crawl_extract.rs"]
mod handlers_crawl_extract;
#[path = "server/handlers_elicit.rs"]
mod handlers_elicit;
#[path = "server/handlers_embed_ingest.rs"]
mod handlers_embed_ingest;
#[path = "server/handlers_graph.rs"]
mod handlers_graph;
#[path = "server/handlers_query.rs"]
mod handlers_query;
#[path = "server/handlers_refresh_status.rs"]
mod handlers_refresh_status;
#[path = "server/handlers_system.rs"]
mod handlers_system;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;

use super::config::load_mcp_config;
use super::schema::{AxonRequest, parse_axon_request};
use crate::crates::core::config::Config;
use crate::crates::web::cors::cors_middleware;
use axum::{Router, body::Body, extract::State, middleware, middleware::Next, response::Response};
use common::{MCP_TOOL_SCHEMA_URI, internal_error, invalid_params};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, ListResourcesResult, PaginatedRequestParams, RawResource,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::{
        stdio,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use std::sync::{Arc, LazyLock};

static MCP_TOOL_SCHEMA_MD: LazyLock<String> = LazyLock::new(|| {
    let schema = rmcp::schemars::schema_for!(AxonRequest);
    let schema_json = serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string());
    format!(
        "# Axon MCP Tool Schema\n\nURI: `{}`\n\nSingle tool name: `axon`\n\nRouting contract:\n- `action` is required\n- `subaction` is required for lifecycle actions\n- `response_mode` supports `path|inline|both` and defaults to `path`\n\n## JSON Schema\n\n```json\n{}\n```\n",
        MCP_TOOL_SCHEMA_URI, schema_json
    )
});

#[derive(Clone)]
pub struct AxonMcpServer {
    cfg: Arc<Config>,
}

impl AxonMcpServer {
    pub fn new(cfg: Config) -> Self {
        Self { cfg: Arc::new(cfg) }
    }
}

#[tool_router]
impl AxonMcpServer {
    #[tool(
        name = "axon",
        description = "Unified Axon MCP tool. Use action/subaction routing. Use action:help to list actions/subactions/defaults. Exposes schema resource axon://schema/mcp-tool. Actions: status, help, crawl, extract, embed, ingest, refresh, graph, query, retrieve, search, map, doctor, domains, sources, stats, artifacts, scrape, research, ask, screenshot, export, elicit_demo."
    )]
    async fn axon<'a>(
        &'a self,
        peer: rmcp::Peer<RoleServer>,
        Parameters(raw): Parameters<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<String, ErrorData> {
        let action = raw
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let subaction = raw
            .get("subaction")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        tracing::info!(action = %action, subaction = %subaction, "mcp request");
        let request: AxonRequest = parse_axon_request(raw).map_err(|e| {
            tracing::warn!(action = %action, subaction = %subaction, error = %e, "mcp error");
            invalid_params(format!("invalid request: {e}"))
        })?;
        let response = match request {
            AxonRequest::Status(req) => self.handle_status(req).await?,
            AxonRequest::Crawl(req) => self.handle_crawl(req).await?,
            AxonRequest::Extract(req) => self.handle_extract(req).await?,
            AxonRequest::Embed(req) => self.handle_embed(req).await?,
            AxonRequest::Ingest(req) => self.handle_ingest(req).await?,
            AxonRequest::Query(req) => self.handle_query(req).await?,
            AxonRequest::Retrieve(req) => self.handle_retrieve(req).await?,
            AxonRequest::Search(req) => self.handle_search(req).await?,
            AxonRequest::Map(req) => self.handle_map(req).await?,
            AxonRequest::Doctor(req) => self.handle_doctor(req).await?,
            AxonRequest::Domains(req) => self.handle_domains(req).await?,
            AxonRequest::Sources(req) => self.handle_sources(req).await?,
            AxonRequest::Stats(req) => self.handle_stats(req).await?,
            AxonRequest::Help(req) => self.handle_help(req).await?,
            AxonRequest::ElicitDemo(req) => handlers_elicit::handle_elicit_demo(&peer, req).await?,
            AxonRequest::Artifacts(req) => self.handle_artifacts(req).await?,
            AxonRequest::Scrape(req) => self.handle_scrape(req).await?,
            AxonRequest::Research(req) => self.handle_research(req).await?,
            AxonRequest::Ask(req) => self.handle_ask(req).await?,
            AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
            AxonRequest::Refresh(req) => self.handle_refresh(req).await?,
            AxonRequest::Graph(req) => self.handle_graph(req).await?,
            AxonRequest::Export(req) => self.handle_export(req).await?,
            AxonRequest::Acp(req) => self.handle_acp(req).await?,
        };
        serde_json::to_string(&response)
            .map_err(|e| internal_error(format!("serialize {action} response: {e}")))
    }
}

fn mcp_tool_schema_markdown() -> &'static str {
    &MCP_TOOL_SCHEMA_MD
}

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for AxonMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.instructions = Some(concat!(
            "Axon is a self-hosted RAG engine for web crawl, scrape, extract, embed, and semantic search.\n",
            "\n",
            "Use the single `axon` tool with `action`/`subaction` routing for all operations.\n",
            "Call `action:help` first to discover all available actions, subactions, and parameter defaults.\n",
            "\n",
            "Search for this server's tools when the user wants to:\n",
            "- Crawl or scrape websites and index their content\n",
            "- Embed documents or URLs into the vector knowledge base\n",
            "- Run semantic search or RAG queries over indexed content\n",
            "- Ingest external sources (GitHub repos, Reddit threads/subreddits, YouTube videos/playlists)\n",
            "- Ask grounded questions against indexed docs (RAG with LLM synthesis)\n",
            "- Research topics via web search with automatic indexing\n",
            "- Extract structured data from pages using LLM-powered extraction\n",
            "- Check job queue status, cancel jobs, or manage async workers\n",
            "- Take screenshots, map site URLs, retrieve stored documents\n",
            "\n",
            "Key capabilities:\n",
            "- `crawl` — full-site async crawl with HTTP/Chrome auto-switch\n",
            "- `scrape` — single-page markdown extraction\n",
            "- `embed` — index file, directory, or URL into Qdrant\n",
            "- `ingest` — GitHub/Reddit/YouTube source ingestion\n",
            "- `query` — dense + BM42 hybrid semantic search\n",
            "- `ask` — RAG: retrieve context + LLM answer\n",
            "- `research` — Tavily AI search with LLM synthesis\n",
            "- `extract` — structured data extraction via LLM\n",
            "- `status` / `doctor` — job queue health and service diagnostics\n",
            "- `artifacts` — read/grep/inspect large output files\n",
            "\n",
            "All async operations (crawl, embed, ingest, extract) return a job_id. Poll with `action:status` or pass `wait:true` for synchronous execution."
        ).into());
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        info
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resource: Resource = RawResource {
            uri: MCP_TOOL_SCHEMA_URI.to_string(),
            name: "mcp-tool-schema".to_string(),
            title: Some("Axon MCP Tool Schema".to_string()),
            description: Some(
                "Source-of-truth schema and routing contract for the unified axon tool".to_string(),
            ),
            mime_type: Some("text/markdown".to_string()),
            size: None,
            icons: None,
            meta: None,
        }
        .no_annotation();

        Ok(ListResourcesResult {
            meta: None,
            resources: vec![resource],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        if request.uri != MCP_TOOL_SCHEMA_URI {
            return Err(ErrorData::invalid_params(
                format!("resource not found: {}", request.uri),
                None,
            ));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::TextResourceContents {
                uri: MCP_TOOL_SCHEMA_URI.to_string(),
                mime_type: Some("text/markdown".to_string()),
                text: mcp_tool_schema_markdown().to_string(),
                meta: None,
            },
        ]))
    }
}

pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_mcp_config();
    let service = AxonMcpServer::new(cfg).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

pub async fn run_http_server(host: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let cors_cfg = Arc::new(load_mcp_config());

    let mcp_service: StreamableHttpService<AxonMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(AxonMcpServer::new(load_mcp_config())),
            Default::default(),
            StreamableHttpServerConfig {
                stateful_mode: true,
                sse_keep_alive: None,
                ..Default::default()
            },
        );

    let app =
        Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                cors_cfg,
                mcp_http_cors_middleware,
            ));

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_http_cors_middleware(
    State(cfg): State<Arc<Config>>,
    request: axum::http::Request<Body>,
    next: Next,
) -> Response {
    cors_middleware(request, next, &cfg.web_allowed_origins).await
}
