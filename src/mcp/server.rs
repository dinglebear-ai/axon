#[path = "server/artifacts.rs"]
pub(super) mod artifacts;
#[path = "server/common.rs"]
pub mod common;
#[path = "server/handler.rs"]
mod handler;
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
#[path = "server/metadata.rs"]
mod metadata;
#[path = "server/scope.rs"]
mod scope;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;

use super::auth::AuthPolicy;
use super::schema::{AxonRequest, parse_axon_request};
use crate::core::config::Config;
use crate::services::context::ServiceContext;
use common::{internal_error, invalid_params};
pub use http::run_unified_server;
use metadata::axon_tool_meta;
use rmcp::{
    ErrorData, RoleServer, ServiceExt, handler::server::wrapper::Parameters, model::CallToolResult,
    tool, tool_handler, tool_router, transport::stdio,
};
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone)]
pub struct AxonMcpServer {
    pub(crate) cfg: Arc<Config>,
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
            tracing::info!(action = %action, subaction = %subaction, dashboard_uri = metadata::STATUS_DASHBOARD_URI, "mcp_app status tool called — widget should render");
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
