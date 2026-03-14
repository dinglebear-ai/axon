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
#[path = "server/handlers_graph.rs"]
mod handlers_graph;
#[path = "server/handlers_query.rs"]
mod handlers_query;
#[path = "server/handlers_refresh_status.rs"]
mod handlers_refresh_status;
#[path = "server/handlers_system.rs"]
mod handlers_system;
#[path = "server/oauth_google.rs"]
mod oauth_google;

#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;

use super::config::load_mcp_config;
use super::schema::{AxonRequest, parse_axon_request};
use crate::crates::core::config::Config;
use crate::crates::web::cors::cors_middleware;
use axum::{
    Router,
    body::Body,
    extract::State,
    middleware,
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use common::{MCP_TOOL_SCHEMA_URI, internal_error, invalid_params};
use oauth_google::{
    GoogleOAuthState, oauth_authorization_server_metadata, oauth_authorization_server_metadata_mcp,
    oauth_authorize, oauth_google_callback, oauth_google_login, oauth_google_logout,
    oauth_google_status, oauth_google_token, oauth_protected_resource_metadata,
    oauth_register_client, oauth_token, require_google_auth,
};
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
use std::sync::Arc;

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
        description = "Unified Axon MCP tool. Use action/subaction routing. Use action:help to list actions/subactions/defaults. Exposes schema resource axon://schema/mcp-tool. Actions: status, help, crawl, extract, embed, ingest, refresh, graph, query, retrieve, search, map, doctor, domains, sources, stats, artifacts, scrape, research, ask, screenshot, elicit_demo."
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
            AxonRequest::Ask(req) => {
                let server = self.clone();
                tokio::task::spawn_blocking(move || {
                    tokio::runtime::Handle::current().block_on(server.handle_ask(req))
                })
                .await
                .map_err(|e| internal_error(format!("ask task join error: {e}")))??
            }
            AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
            AxonRequest::Refresh(req) => self.handle_refresh(req).await?,
            AxonRequest::Graph(req) => self.handle_graph(req).await?,
        };
        serde_json::to_string(&response).map_err(|e| internal_error(e.to_string()))
    }
}

fn mcp_tool_schema_markdown() -> String {
    let schema = rmcp::schemars::schema_for!(AxonRequest);
    let schema_json = serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string());
    format!(
        "# Axon MCP Tool Schema\n\nURI: `{}`\n\nSingle tool name: `axon`\n\nRouting contract:\n- `action` is required\n- `subaction` is required for lifecycle actions\n- `response_mode` supports `path|inline|both` and defaults to `path`\n\n## JSON Schema\n\n```json\n{}\n```\n",
        MCP_TOOL_SCHEMA_URI, schema_json
    )
}

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for AxonMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "Use the single axon tool with action/subaction to drive crawl and RAG workflows"
                .into(),
        );
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
                text: mcp_tool_schema_markdown(),
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
    let oauth_state = GoogleOAuthState::from_env(host, port);
    let oauth_state_for_layer = oauth_state.clone();

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

    let app = Router::new()
        .nest_service("/mcp", mcp_service)
        .route("/oauth/google/status", get(oauth_google_status))
        .route("/oauth/google/login", get(oauth_google_login))
        .route("/oauth/google/callback", get(oauth_google_callback))
        .route("/oauth/google/token", get(oauth_google_token))
        .route(
            "/oauth/google/logout",
            get(oauth_google_logout).post(oauth_google_logout),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_authorization_server_metadata),
        )
        .route(
            // Path-prefix discovery alias for the /mcp resource (RFC 8414 §3.1).
            // Uses a dedicated handler that returns issuer = resource_server_url so the
            // issuer matches the request path — the root handler would return the broker
            // issuer which would fail RFC 8414 §3 validation for MCP clients.
            "/.well-known/oauth-authorization-server/mcp",
            get(oauth_authorization_server_metadata_mcp),
        )
        .route(
            "/.well-known/openid-configuration",
            get(oauth_authorization_server_metadata),
        )
        .route(
            "/.well-known/openid-configuration/mcp",
            get(oauth_authorization_server_metadata),
        )
        .route("/oauth/register", post(oauth_register_client))
        .route("/oauth/authorize", get(oauth_authorize))
        .route("/oauth/token", post(oauth_token))
        .with_state(oauth_state)
        .layer(middleware::from_fn_with_state(
            oauth_state_for_layer,
            require_google_auth,
        ))
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
