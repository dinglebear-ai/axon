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

use super::schema::{AxonRequest, parse_axon_request};
use crate::crates::core::config::Config;
use crate::crates::services::context::ServiceContext;
use crate::crates::web::cors::cors_middleware;
use axum::{Router, body::Body, extract::State, middleware, middleware::Next, response::Response};
use common::{MCP_TOOL_SCHEMA_URI, internal_error, invalid_params};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, ExtensionCapabilities, InitializeRequestParams, InitializeResult,
        ListResourcesResult, Meta, PaginatedRequestParams, RawResource, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo,
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
use tokio::sync::OnceCell;

const STATUS_DASHBOARD_URI: &str = "ui://axon/status-dashboard";
const MCP_APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
static STATUS_DASHBOARD_HTML: &str = include_str!("assets/status_dashboard.html");

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
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
}

impl AxonMcpServer {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
            service_context: Arc::new(OnceCell::new()),
        }
    }

    pub(super) async fn base_service_context(
        &self,
    ) -> Result<Arc<ServiceContext>, Box<dyn std::error::Error + Send + Sync>> {
        self.service_context
            .get_or_try_init(|| async {
                ServiceContext::new_with_workers(Arc::clone(&self.cfg))
                    .await
                    .map(Arc::new)
            })
            .await
            .map(Arc::clone)
    }

    pub(super) async fn service_context_for(
        &self,
        cfg: Config,
    ) -> Result<ServiceContext, Box<dyn std::error::Error + Send + Sync>> {
        let base = self.base_service_context().await?;
        Ok(ServiceContext::from_runtime(
            Arc::new(cfg),
            Arc::clone(&base.jobs),
        ))
    }
}

#[tool_router]
impl AxonMcpServer {
    #[tool(
        name = "axon",
        description = "Unified Axon MCP tool. Use action/subaction routing. Use action:help to list actions/subactions/defaults. Exposes schema resource axon://schema/mcp-tool. Actions: status, help, crawl, extract, embed, ingest, refresh, graph, query, retrieve, search, map, doctor, domains, sources, stats, artifacts, scrape, research, ask, screenshot, export, elicit_demo.",
        meta = axon_tool_meta()
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
        if action == "status" {
            tracing::info!(action = %action, subaction = %subaction, dashboard_uri = STATUS_DASHBOARD_URI, "mcp_app status tool called — widget should render");
        }
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

fn axon_tool_meta() -> Meta {
    let mut m = Meta::new();
    // Nested form: _meta.ui.resourceUri (TypeScript SDK / MCP Apps convention).
    // The MCP host SDK normalizes this into a flat key internally; we only need
    // to emit the canonical nested form here.
    m.insert(
        "ui".to_string(),
        serde_json::json!({ "resourceUri": STATUS_DASHBOARD_URI }),
    );
    m
}

fn mcp_apps_server_capabilities() -> ServerCapabilities {
    let mut extensions = ExtensionCapabilities::new();
    // Declare MCP Apps extension support so the host knows to render widgets.
    let mut ui_ext = serde_json::Map::new();
    ui_ext.insert(
        "mimeTypes".to_string(),
        serde_json::json!(["text/html;profile=mcp-app"]),
    );
    extensions.insert("io.modelcontextprotocol/ui".to_string(), ui_ext);
    ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .enable_extensions_with(extensions)
        .build()
}

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for AxonMcpServer {
    async fn initialize(
        &self,
        request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        tracing::info!(
            client_name = %request.client_info.name,
            client_version = %request.client_info.version,
            protocol_version = %request.protocol_version,
            has_extensions = %request.capabilities.extensions.is_some(),
            extensions = ?request.capabilities.extensions,
            "mcp_app initialize — client capabilities"
        );
        let info = self.get_info();
        Ok(InitializeResult::new(info.capabilities)
            .with_protocol_version(request.protocol_version)
            .with_server_info(info.server_info)
            .with_instructions(info.instructions.unwrap_or_default()))
    }

    fn get_info(&self) -> ServerInfo {
        tracing::info!("mcp_app get_info called — client connected");
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
            "- MCP Apps enabled — exposes `ui://axon/status-dashboard` for live queue status widgets\n",
            "\n",
            "All async operations (crawl, embed, ingest, extract) return a job_id. Poll with `action:status` or pass `wait:true` for synchronous execution."
        ).into());
        info.capabilities = mcp_apps_server_capabilities();
        info
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        tracing::info!("mcp_app list_resources called");
        let schema_resource: Resource = RawResource {
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

        let dashboard_resource: Resource = RawResource {
            uri: STATUS_DASHBOARD_URI.to_string(),
            name: "status-dashboard".to_string(),
            title: Some("Axon Status Dashboard".to_string()),
            description: Some(
                "Interactive MCP App widget showing live job queue status for all Axon workers"
                    .to_string(),
            ),
            mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
            size: None,
            icons: None,
            meta: None,
        }
        .no_annotation();

        Ok(ListResourcesResult {
            meta: None,
            resources: vec![schema_resource, dashboard_resource],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        tracing::info!(uri = %request.uri, "mcp_app read_resource called");
        if request.uri == STATUS_DASHBOARD_URI {
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::TextResourceContents {
                    uri: STATUS_DASHBOARD_URI.to_string(),
                    mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
                    text: STATUS_DASHBOARD_HTML.to_string(),
                    meta: None,
                },
            ]));
        }
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

pub async fn run_stdio_server(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    let service = AxonMcpServer::new(cfg).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

pub async fn run_http_server(
    cfg: Config,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let cors_cfg = Arc::new(cfg.clone());
    let cfg_arc = Arc::new(cfg);

    let mcp_service: StreamableHttpService<AxonMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(AxonMcpServer::new((*cfg_arc).clone())),
            Default::default(),
            {
                let mut cfg = StreamableHttpServerConfig::default();
                cfg.stateful_mode = true;
                cfg
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
