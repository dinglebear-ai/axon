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
#[path = "server/http.rs"]
mod http;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;

use super::auth::AuthPolicy;
use super::schema::{AxonRequest, parse_axon_request};
use crate::core::config::Config;
use crate::services::context::ServiceContext;
use crate::services::system;
use common::{MCP_TOOL_SCHEMA_URI, internal_error, invalid_params};
pub use http::run_unified_server;
use lab_auth::AuthContext;
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{
        AnnotateAble, CallToolRequestParams, CallToolResult, ExtensionCapabilities,
        InitializeRequestParams, InitializeResult, ListResourcesResult, Meta,
        PaginatedRequestParams, RawResource, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
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
        "# Axon MCP Tool Schema\n\nURI: `{}`\n\nSingle tool name: `axon`\n\nRouting contract:\n- `action` is required\n- `subaction` is required for subaction families\n- `response_mode` supports `path|inline|both|auto_inline`; most actions default to `path`, while `scrape` and `retrieve` default to inline paged document reads\n\n## JSON Schema\n\n```json\n{}\n```\n",
        MCP_TOOL_SCHEMA_URI, schema_json
    )
});

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
        description = "Unified Axon MCP tool. Use action/subaction routing. Use action:help to list actions/subactions/defaults. Exposes schema resource axon://schema/mcp-tool. Actions: status, help, crawl, extract, embed, ingest, query, retrieve, search, map, evaluate, suggest, doctor, domains, sources, stats, artifacts, scrape, research, ask, screenshot, elicit_demo.",
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
            AxonRequest::Research(req) => self.handle_research(req).await?,
            AxonRequest::Ask(req) => self.handle_ask(req).await?,
            AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
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

// ── Scope checking ────────────────────────────────────────────────────────────

/// Extract and enforce the authentication context from the rmcp request.
///
/// - `AuthPolicy::LoopbackDev`: always returns `Ok(None)` — the loopback bind
///   is the trust boundary; no per-request credential needed.
/// - `AuthPolicy::Mounted(_)`: the middleware MUST have inserted an
///   [`AuthContext`] into the request extensions. If it is absent, this
///   returns a forbidden error immediately (fail-closed).
///
/// Returns `Ok(Some(&AuthContext))` for Mounted+present, `Ok(None)` for
/// LoopbackDev.
fn require_auth_context<'a>(
    policy: &AuthPolicy,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match policy {
        AuthPolicy::LoopbackDev => Ok(None),
        AuthPolicy::Mounted { .. } => {
            let parts = ctx
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| {
                    // Framework-level invariant violation: rmcp changed how it
                    // propagates HTTP Parts, or middleware ordering is broken.
                    tracing::error!(
                        "rmcp HTTP Parts extension absent — middleware ordering may be broken"
                    );
                    ErrorData::invalid_request("forbidden: missing http context", None)
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                // AuthLayer should always insert AuthContext on the happy path.
                tracing::warn!(
                    "AuthContext absent from request extensions — \
                     AuthLayer may not be mounted or rejected the request without inserting context"
                );
                ErrorData::invalid_request("forbidden: missing auth context", None)
            })?;
            Ok(Some(auth))
        }
    }
}

/// Enforce that `auth` carries `required_scope`.
///
/// `axon:write` is treated as a superset of `axon:read` — a caller with write
/// access implicitly satisfies any read-level scope requirement.
fn check_scope(auth: &AuthContext, required_scope: &str, action: &str) -> Result<(), ErrorData> {
    let satisfied = auth
        .scopes
        .iter()
        .any(|s| s == required_scope || (required_scope == "axon:read" && s == "axon:write"));
    if satisfied {
        return Ok(());
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "MCP tool invocation denied: insufficient scope"
    );
    Err(ErrorData::invalid_request(
        format!("forbidden: requires scope: {required_scope}"),
        None,
    ))
}

/// Map an axon tool action (and optional sub-action) to the minimum required scope.
///
/// Returns `None` for informational actions that need `AuthContext` (when
/// Mounted) but no specific scope gate — e.g. `help`.
/// Unknown actions return `Some("__deny__")` — a sentinel that `call_tool`
/// treats as an explicit deny rather than skipping the check. This is
/// fail-conservative: new actions added without a mapping entry are denied
/// rather than accidentally permitted.
///
/// Note on `"artifacts"`: sub-actions `delete` and `clean` require
/// `axon:write`; all others require `axon:read`. The caller passes the
/// `subaction` field from the parsed request arguments.
///
/// Note on `"scrape"`: scrape crawls and stores content — it is a write
/// operation and requires `axon:write`.
fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
    match action {
        // Informational — AuthContext required when Mounted, but no scope gate.
        "help" => None,
        // Write/mutating operations require axon:write.
        "crawl" | "extract" | "embed" | "ingest" | "scrape" => Some("axon:write"),
        // artifacts: write subactions need axon:write, read subactions need axon:read.
        "artifacts" => match subaction {
            "delete" | "clean" => Some("axon:write"),
            _ => Some("axon:read"),
        },
        // Read / query operations require axon:read.
        "status" | "query" | "retrieve" | "search" | "map" | "evaluate" | "suggest" | "doctor"
        | "domains" | "sources" | "stats" | "research" | "ask" | "screenshot" => Some("axon:read"),
        // Unknown actions are explicitly denied (fail-conservative). Add an
        // explicit mapping above for any new action before shipping.
        _ => Some("__deny__"),
    }
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
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
        let subaction: String = request
            .arguments
            .as_ref()
            .and_then(|m| m.get("subaction"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();

        // Fail-closed auth check: require AuthContext when Mounted, then scope.
        // LoopbackDev returns None — no scope enforcement applies.
        let auth = require_auth_context(&self.auth_policy, &context)?;
        match (auth, required_scope_for(&action, &subaction)) {
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
                check_scope(auth_ctx, required_scope, &action)?;
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
    // Stdio always uses LoopbackDev: process isolation is the trust boundary.
    let server = AxonMcpServer::new(cfg).with_auth_policy(AuthPolicy::LoopbackDev);
    server
        .base_service_context()
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
