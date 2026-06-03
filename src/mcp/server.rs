#[path = "server/artifacts.rs"]
pub(super) mod artifacts;
#[path = "server/common.rs"]
pub mod common;
#[path = "server/handlers_crawl_extract.rs"]
mod handlers_crawl_extract;
#[path = "server/handlers_elicit.rs"]
mod handlers_elicit;
#[path = "server/handlers_embed_ingest.rs"]
mod handlers_embed_ingest;
#[path = "server/handlers_query.rs"]
mod handlers_query;
#[path = "server/handlers_system.rs"]
mod handlers_system;
#[path = "server/handlers_vertical_scrape.rs"]
mod handlers_vertical_scrape;
#[path = "server/http.rs"]
mod http;
#[path = "server/authz.rs"]
mod server_authz;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;
#[path = "server/stdio.rs"]
mod stdio_runner;
#[path = "server/tool_schema.rs"]
mod tool_schema;
#[cfg(test)]
#[path = "server/tool_schema_tests.rs"]
mod tool_schema_tests;

use super::auth::AuthPolicy;
use super::schema::{AxonRequest, parse_axon_request};
use super::thin_client;
use crate::core::config::Config;
use crate::services::context::ServiceContext;
use crate::services::system;
use common::{MCP_TOOL_SCHEMA_URI, internal_error, invalid_params};
pub use http::run_unified_server;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, CallToolRequestParams, CallToolResult, ExtensionCapabilities,
        InitializeRequestParams, InitializeResult, ListResourcesResult, Meta,
        PaginatedRequestParams, RawResource, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde_json::Value;
pub use server_authz::required_scope_for;
use server_authz::required_scope_for_tool;
use std::sync::{Arc, LazyLock};
pub use stdio_runner::run_stdio_server;
use tokio::sync::OnceCell;

const STATUS_DASHBOARD_URI: &str = "ui://axon/status-dashboard";
const MCP_APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
static STATUS_DASHBOARD_HTML: &str = include_str!("assets/status_dashboard.html");

static MCP_TOOL_SCHEMA_MD: LazyLock<String> = LazyLock::new(tool_schema::mcp_tool_schema_markdown);

#[derive(Clone)]
pub struct AxonMcpServer {
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    /// Authentication policy for this server instance.
    ///
    /// Set to `LoopbackDev` for stdio mode (process isolation is the trust
    /// boundary). Set to `Mounted { .. }` when the HTTP server is started
    /// with auth enabled. The policy is cloned into each server instance
    /// created by the `StreamableHttpService` factory closure.
    pub(crate) auth_policy: AuthPolicy,
}

impl AxonMcpServer {
    pub fn new(cfg: Config) -> Self {
        // Default to LoopbackDev; the HTTP server overrides this via
        // `new_with_auth_policy` when auth is configured.
        Self {
            cfg: Arc::new(cfg),
            service_context: Arc::new(OnceCell::new()),
            auth_policy: AuthPolicy::LoopbackDev,
        }
    }

    fn new_with_service_context_cell(
        cfg: Config,
        service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    ) -> Self {
        Self {
            cfg: Arc::new(cfg),
            service_context,
            auth_policy: AuthPolicy::LoopbackDev,
        }
    }

    pub(super) fn with_auth_policy(mut self, auth_policy: AuthPolicy) -> Self {
        self.auth_policy = auth_policy;
        self
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
        description = "Unified Axon MCP tool. Use action/subaction routing. Valid actions and subactions are published in this tool inputSchema and mirrored in the enriched schema resource at axon://schema/mcp-tool. Actions: status, help, crawl, extract, embed, ingest, query, retrieve, search, map, endpoints, evaluate, suggest, doctor, domains, sources, stats, artifacts, scrape, research, ask, summarize, screenshot, elicit_demo, brand, diff.",
        input_schema = tool_schema::axon_tool_input_schema()
    )]
    async fn axon<'a>(
        &'a self,
        peer: rmcp::Peer<RoleServer>,
        Parameters(raw): Parameters<serde_json::Map<String, Value>>,
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
        if let Some(response) = thin_client::route_request(self.cfg.as_ref(), &request)
            .await
            .map_err(map_thin_client_error)?
        {
            return serde_json::to_string(&response)
                .map_err(|e| internal_error(format!("serialize {action} response: {e}")));
        }
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
            AxonRequest::Endpoints(req) => self.handle_endpoints(req).await?,
            AxonRequest::Evaluate(req) => self.handle_evaluate(req).await?,
            AxonRequest::Suggest(req) => self.handle_suggest(req).await?,
            AxonRequest::Doctor(req) => self.handle_doctor(req).await?,
            AxonRequest::Domains(req) => self.handle_domains(req).await?,
            AxonRequest::Sources(req) => self.handle_sources(req).await?,
            AxonRequest::Stats(req) => self.handle_stats(req).await?,
            AxonRequest::Help(req) => self.handle_help(req).await?,
            AxonRequest::ElicitDemo(req) => handlers_elicit::handle_elicit_demo(&peer, req).await?,
            AxonRequest::Artifacts(req) => self.handle_artifacts(req).await?,
            AxonRequest::Scrape(req) => self.handle_scrape(req).await?,
            AxonRequest::VerticalScrape(req) => self.handle_vertical_scrape(req).await?,
            AxonRequest::Research(req) => self.handle_research(req).await?,
            AxonRequest::Ask(req) => self.handle_ask(req).await?,
            AxonRequest::Summarize(req) => self.handle_summarize(req).await?,
            AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
            AxonRequest::Diff(req) => self.handle_diff(req).await?,
            AxonRequest::Brand(req) => self.handle_brand(req).await?,
            AxonRequest::Debug(_)
            | AxonRequest::Dedupe(_)
            | AxonRequest::Migrate(_)
            | AxonRequest::Watch(_)
            | AxonRequest::Setup(_) => {
                return Err(invalid_params(
                    "this action is available through the HTTP API, not MCP",
                ));
            }
        };
        serde_json::to_string(&response)
            .map_err(|e| internal_error(format!("serialize {action} response: {e}")))
    }

    #[tool(
        name = "axon_status_dashboard",
        description = "Render Axon's interactive MCP Apps status dashboard. Use this when the user wants to inspect live crawl, embed, extract, ingest, worker, and service status visually.",
        meta = status_dashboard_tool_meta()
    )]
    async fn axon_status_dashboard(&self) -> Result<CallToolResult, ErrorData> {
        tracing::info!(
            dashboard_uri = STATUS_DASHBOARD_URI,
            "mcp_app dedicated status dashboard tool called"
        );
        let ctx = ServiceContext::new(self.cfg.clone())
            .await
            .map_err(|e| internal_error(format!("initialize status dashboard context: {e}")))?;
        let status = system::full_status(&ctx)
            .await
            .map_err(|e| internal_error(format!("load status dashboard data: {e}")))?;
        let structured = serde_json::to_value(&status.payload)
            .map_err(|e| internal_error(format!("serialize status dashboard payload: {e}")))?;
        Ok(CallToolResult::structured(structured))
    }
}

fn map_thin_client_error(err: thin_client::ThinClientError) -> ErrorData {
    match err {
        thin_client::ThinClientError::InvalidRequest(message) => invalid_params(message),
        other => internal_error(format!("MCP thin client route failed: {other}")),
    }
}

fn mcp_tool_schema_markdown() -> &'static str {
    &MCP_TOOL_SCHEMA_MD
}

fn status_dashboard_tool_meta() -> Meta {
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

fn status_dashboard_resource_meta() -> Meta {
    let mut m = Meta::new();
    m.insert(
        "ui".to_string(),
        serde_json::json!({
            "csp": {
                "connectDomains": [],
                "resourceDomains": [],
                "frameDomains": [],
                "baseUriDomains": []
            },
            "permissions": {}
        }),
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
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // Extract action and subaction for scope check before any processing.
        let action: String = request
            .arguments
            .as_ref()
            .and_then(|m| m.get("action"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let subaction: String = request
            .arguments
            .as_ref()
            .and_then(|m| m.get("subaction"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        // Fail-closed auth check: require AuthContext when Mounted, then scope.
        // LoopbackDev returns None — no scope enforcement applies.
        let auth = server_authz::require_auth_context(&self.auth_policy, &context)?;
        match (
            auth,
            required_scope_for_tool(request.name.as_ref(), &action, &subaction),
        ) {
            // Deny: sentinel returned for unknown actions — even with a valid
            // token, we refuse rather than accidentally granting access.
            (Some(_), Some("__deny__")) => {
                tracing::warn!(
                    action = %action,
                    "MCP tool invocation denied: unknown action (fail-conservative)"
                );
                return Err(ErrorData::invalid_request(
                    format!("forbidden: unknown action `{action}`"),
                    None,
                ));
            }
            // No scope required (e.g. "help") — allowed through when authenticated.
            (Some(_), None) => {}
            // Scope check required.
            (Some(auth_ctx), Some(required_scope)) => {
                server_authz::check_scope(auth_ctx, required_scope, &action)?;
            }
            // LoopbackDev — no enforcement.
            (None, _) => {}
        }

        // Delegate to the tool router generated by #[tool_router].
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }

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
            "- Summarize one or more URLs from freshly scraped page context\n",
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
            "- `endpoints` — static endpoint discovery with optional verification\n",
            "- `ask` — RAG: retrieve context + LLM answer\n",
            "- `summarize` — scrape URL context + configured LLM summary\n",
            "- `evaluate` — compare RAG quality against a baseline with judge diagnostics\n",
            "- `suggest` — propose new crawl targets from indexed source coverage\n",
            "- `research` — Tavily AI search with LLM synthesis\n",
            "- `extract` — structured data extraction via LLM\n",
            "- `status` / `doctor` — job queue health and service diagnostics\n",
            "- `artifacts` — read/grep/inspect large output files\n",
            "- MCP Apps enabled — exposes `ui://axon/status-dashboard` for live queue status widgets\n",
            "\n",
            "Async operations (crawl, embed, ingest, extract) return a job_id. Poll the same action with `subaction:status` and the returned `job_id`."
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
            meta: Some(status_dashboard_resource_meta()),
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
            // Inject current status data so the widget renders immediately, bypassing
            // the MCP Apps postMessage bridge which may not be available in all hosts.
            let status_json = match ServiceContext::new(self.cfg.clone()).await {
                Ok(ctx) => match system::full_status(&ctx).await {
                    Ok(r) => {
                        serde_json::to_string(&r.payload).unwrap_or_else(|_| "null".to_string())
                    }
                    Err(_) => "null".to_string(),
                },
                Err(_) => "null".to_string(),
            };
            let html = STATUS_DASHBOARD_HTML.replacen(
                "window.__AXON_INITIAL_STATUS__ = null;",
                &format!("window.__AXON_INITIAL_STATUS__ = {};", status_json),
                1,
            );
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::TextResourceContents {
                    uri: STATUS_DASHBOARD_URI.to_string(),
                    mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
                    text: html,
                    meta: Some(status_dashboard_resource_meta()),
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
