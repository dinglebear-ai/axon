#[path = "server/artifacts.rs"]
pub(super) mod artifacts;
#[path = "server/common.rs"]
pub mod common;
#[path = "server/handler_meta.rs"]
mod handler_meta;
#[path = "server/handlers_elicit.rs"]
mod handlers_elicit;
#[path = "server/handlers_extract.rs"]
mod handlers_extract;
#[path = "server/handlers_jobs.rs"]
mod handlers_jobs;
#[path = "server/handlers_memory.rs"]
mod handlers_memory;
#[path = "server/handlers_query.rs"]
mod handlers_query;
#[path = "server/handlers_source.rs"]
mod handlers_source;
#[path = "server/handlers_system.rs"]
mod handlers_system;
#[path = "server/http.rs"]
mod http;
#[path = "server/authz.rs"]
mod server_authz;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;
#[path = "server/stdio.rs"]
mod stdio_runner;
#[path = "server/task_id.rs"]
mod task_id;
#[path = "server/task_progress.rs"]
mod task_progress;
#[path = "server/task_status.rs"]
mod task_status;
#[path = "server/tasks.rs"]
mod tasks;
#[path = "server/tool_schema.rs"]
mod tool_schema;
#[cfg(test)]
#[path = "server/tool_schema_tests.rs"]
mod tool_schema_tests;

use super::auth::AuthPolicy;
use super::schema::{AxonRequest, parse_axon_request};
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::system;
use common::{internal_error, invalid_params};
use handler_meta::STATUS_DASHBOARD_URI;
pub use http::mcp_http_router;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CreateTaskResult, GetTaskInfoParams, GetTaskPayloadResult, GetTaskResult,
        GetTaskResultParams, InitializeRequestParams, InitializeResult, ListResourcesResult,
        ListTasksResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde_json::Value;
pub use server_authz::required_scope_for;
use server_authz::required_scope_for_tool;
use std::{collections::HashMap, sync::Arc};
pub use stdio_runner::run_stdio_server;
use tokio::{
    sync::{Mutex, OnceCell},
    task::JoinHandle,
};

#[derive(Clone)]
pub struct AxonMcpServer {
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    progress_notifiers: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
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
            progress_notifiers: Arc::new(Mutex::new(HashMap::new())),
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
            progress_notifiers: Arc::new(Mutex::new(HashMap::new())),
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
                ServiceContext::new_with_workers_and_schedulers(Arc::clone(&self.cfg))
                    .await
                    .map(Arc::new)
            })
            .await
            .map(Arc::clone)
    }
}

#[tool_router]
impl AxonMcpServer {
    #[tool(
        name = "axon",
        description = "Unified Axon MCP tool. Use action/subaction routing. Valid actions and subactions are published in this tool inputSchema and mirrored in the enriched schema resource at axon://schema/mcp-tool. Actions: status, help, source, extract, memory, query, retrieve, search, map, endpoints, evaluate, suggest, doctor, domains, sources, stats, research, ask, summarize, screenshot, elicit_demo, brand, diff, purge. The single `source` action indexes any local path, git/web/feed/youtube/reddit/session/registry target (replaces the former embed/ingest/scrape/crawl/code_search/vertical_scrape actions).",
        input_schema = tool_schema::axon_tool_input_schema(),
        execution(task_support = "optional")
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
        let response = match request {
            AxonRequest::Status(req) => self.handle_status(req).await?,
            AxonRequest::Jobs(req) => self.handle_jobs(req).await?,
            AxonRequest::Source(req) => self.handle_source(req).await?,
            AxonRequest::Extract(req) => self.handle_extract(req).await?,
            AxonRequest::Memory(req) => self.handle_memory(req).await?,
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
            AxonRequest::Research(req) => self.handle_research(req).await?,
            AxonRequest::Ask(req) => self.handle_ask(req).await?,
            AxonRequest::Summarize(req) => self.handle_summarize(req).await?,
            AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
            AxonRequest::Diff(req) => self.handle_diff(req).await?,
            AxonRequest::Brand(req) => self.handle_brand(req).await?,
            AxonRequest::Purge(req) => self.handle_purge(req).await?,
            AxonRequest::Prune(req) => self.handle_prune(req).await?,
            // Removed indexing actions: `embed`, `ingest`, `scrape`, `crawl`,
            // `code_search`, and `vertical_scrape` are folded into `source`.
            // These variants remain on the shared `AxonRequest` for the REST
            // surface, but the MCP authz allow-list rejects them before
            // dispatch; the arm here keeps the match exhaustive and gives a
            // clear message if one is ever reached.
            AxonRequest::Embed(_)
            | AxonRequest::Ingest(_)
            | AxonRequest::Scrape(_)
            | AxonRequest::Crawl(_)
            | AxonRequest::CodeSearch(_)
            | AxonRequest::VerticalScrape(_) => {
                return Err(invalid_params(
                    "this action was removed from MCP; use action=source to index any local path, \
                     git/web/feed/youtube/reddit/session/registry target",
                ));
            }
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
        let response = handler_meta::append_stale_binary_warning(response);
        serde_json::to_string(&response)
            .map_err(|e| internal_error(format!("serialize {action} response: {e}")))
    }

    #[tool(
        name = "axon_status_dashboard",
        description = "Render Axon's interactive MCP Apps status dashboard. Use this when the user wants to inspect live crawl, embed, extract, ingest, worker, and service status visually.",
        meta = handler_meta::status_dashboard_tool_meta()
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

        // `prune` needs a real PruneAuthz derived from the caller's resolved
        // scopes, never hardcoded. By the time we reach this point the scope
        // gate above has already enforced `axon:admin` for Mounted callers
        // (see server_authz::MCP_ACTION_SPECS / required_scope_for), so
        // re-deriving `is_admin` here from the same `auth` value is honest,
        // not a rubber stamp — LoopbackDev has no AuthContext at all and is
        // treated as locally-trusted, matching the CLI's own local-trust
        // rationale (crates/axon-cli/src/commands/prune.rs).
        let prune_authz = if action == "prune" {
            axon_services::prune::PruneAuthz {
                is_admin: match auth {
                    None => true,
                    Some(auth_ctx) => {
                        axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE)
                    }
                },
            }
        } else {
            axon_services::prune::PruneAuthz::anonymous()
        };

        // Delegate to the tool router generated by #[tool_router], with the
        // resolved prune authz available to `handle_prune` via task-local.
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        common::CURRENT_PRUNE_AUTHZ
            .scope(prune_authz, Self::tool_router().call(tcc))
            .await
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, ErrorData> {
        tasks::enqueue_task(self, request, context).await
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, ErrorData> {
        tasks::list_tasks(self, request, context).await
    }

    async fn get_task_info(
        &self,
        request: GetTaskInfoParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        tasks::get_task_info(self, request, context).await
    }

    async fn get_task_result(
        &self,
        request: GetTaskResultParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, ErrorData> {
        tasks::get_task_result(self, request, context).await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, ErrorData> {
        tasks::cancel_task(self, request, context).await
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        handler_meta::initialize(self, request).await
    }

    fn get_info(&self) -> ServerInfo {
        handler_meta::get_info(self)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        handler_meta::list_resources(self, request, context).await
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        handler_meta::read_resource(self, request, context).await
    }
}
