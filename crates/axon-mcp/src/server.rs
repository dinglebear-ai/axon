#[path = "server/artifacts.rs"]
pub(super) mod artifacts;
#[path = "server/common.rs"]
pub mod common;
#[path = "server/handler_meta.rs"]
mod handler_meta;
#[path = "server/handlers_discovery.rs"]
mod handlers_discovery;
#[path = "server/handlers_extract.rs"]
mod handlers_extract;
#[path = "server/handlers_graph.rs"]
mod handlers_graph;
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
#[path = "server/handlers_watch.rs"]
mod handlers_watch;
#[path = "server/http.rs"]
mod http;
#[path = "server/authz.rs"]
mod server_authz;
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;
#[path = "server/stdio.rs"]
mod stdio_runner;
#[path = "server/system_requests.rs"]
mod system_requests;
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

use self::system_requests::{McpSystemRequest, McpWatchRequest};
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
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams, GetTaskParams,
        GetTaskResult, InitializeRequestParams, InitializeResult, ListResourcesResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, RequestMetaObject,
        ServerInfo, TASKS_EXTENSION_ID, UpdateTaskParams,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde_json::Value;
pub use server_authz::{mutates_if_upgrade, required_scope_for, required_scope_with_mutates_if};
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
        description = "Unified Axon MCP tool. Use action/subaction routing. Actions: help, status, jobs, doctor, source, query, retrieve, resolve, capabilities, providers, search, map, prune, collections, reset, ask, chat, evaluate, suggest, research, screenshot, brand, diff, extract, memory, summarize, endpoints, watch, graph, uploads, artifacts. Valid subactions are published in this tool inputSchema and mirrored in the enriched schema resource at axon://schema/mcp-tool. Uploads use distinct upl_* staging IDs and art_* artifact IDs. The `source` action indexes any supported source through the unified pipeline.",
        input_schema = tool_schema::axon_tool_input_schema()
    )]
    async fn axon<'a>(
        &'a self,
        // No dispatch arm currently needs the live MCP peer.
        _peer: rmcp::Peer<RoleServer>,
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
        let response = if matches!(
            action.as_str(),
            "reset" | "collections" | "uploads" | "artifacts"
        ) {
            let request: McpSystemRequest = serde_json::from_value(Value::Object(raw)).map_err(|e| {
                tracing::warn!(action = %action, subaction = %subaction, error = %e, "mcp error");
                invalid_params(format!("invalid request: {e}"))
            })?;
            match request {
                McpSystemRequest::Reset(req) => self.handle_reset(req).await?,
                McpSystemRequest::Collections(req) => self.handle_collections(req).await?,
                McpSystemRequest::Uploads(req) => self.handle_uploads(req).await?,
                McpSystemRequest::Artifacts(req) => self.handle_artifacts(req).await?,
            }
        } else if action == "watch" {
            let request: McpWatchRequest = serde_json::from_value(Value::Object(raw)).map_err(|e| {
                tracing::warn!(action = %action, subaction = %subaction, error = %e, "mcp error");
                invalid_params(format!("invalid request: {e}"))
            })?;
            let McpWatchRequest::Watch(req) = request;
            self.handle_watch(req).await?
        } else {
            let request: AxonRequest = parse_axon_request(raw).map_err(|e| {
                tracing::warn!(action = %action, subaction = %subaction, error = %e, "mcp error");
                invalid_params(format!("invalid request: {e}"))
            })?;
            match request {
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
                AxonRequest::Help(req) => self.handle_help(req).await?,
                AxonRequest::Resolve(req) => self.handle_resolve(req).await?,
                AxonRequest::Capabilities(req) => self.handle_capabilities(req).await?,
                AxonRequest::Providers(req) => self.handle_providers(req).await?,
                // `sources`, `domains`, and `stats` are removed
                // from the MCP surface per the tool contract (issue #298 WS-G):
                // `sources`/`domains` have no contracted equivalent yet (tracked
                // as a WS-G followup), `stats` folds toward `action=collections`
                // once a real CollectionService backs it (also a followup), and
                // contract's canonical list. These remain on the shared
                // `AxonRequest` enum for REST/CLI compatibility, but MCP authz
                // (`MCP_ACTION_SPECS`) already denies them before dispatch; this
                // arm keeps the match exhaustive and gives a clear message for
                // LoopbackDev callers that skip the authz gate.
                AxonRequest::Sources(_) | AxonRequest::Domains(_) | AxonRequest::Stats(_) => {
                    return Err(invalid_params(
                        "this action was removed from MCP; use action=query/retrieve for indexed \
                     content lookups, or action=doctor for service health",
                    ));
                }
                AxonRequest::Research(req) => self.handle_research(req).await?,
                AxonRequest::Ask(req) => self.handle_ask(req).await?,
                AxonRequest::Summarize(req) => self.handle_summarize(req).await?,
                AxonRequest::Screenshot(req) => self.handle_screenshot(req).await?,
                AxonRequest::Diff(req) => self.handle_diff(req).await?,
                AxonRequest::Brand(req) => self.handle_brand(req).await?,
                AxonRequest::Prune(req) => self.handle_prune(req).await?,
                AxonRequest::Watch(_) => {
                    return Err(invalid_params(
                        "watch requests must use the canonical MCP watch DTO",
                    ));
                }
                AxonRequest::Graph(req) => self.handle_graph(req).await?,
                AxonRequest::Chat(req) => self.handle_chat(req).await?,
                AxonRequest::Debug(_) | AxonRequest::Migrate(_) | AxonRequest::Setup(_) => {
                    return Err(invalid_params(
                        "this action is available through the HTTP API, not MCP",
                    ));
                }
            }
        };
        let response = handler_meta::append_stale_binary_warning(response);
        serde_json::to_string(&response)
            .map_err(|e| internal_error(format!("serialize {action} response: {e}")))
    }

    #[tool(
        name = "axon_status_dashboard",
        description = "Render Axon's interactive MCP Apps status dashboard. Use this when the user wants to inspect live source, extract, worker, and service status visually.",
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
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // rmcp strips wire `params._meta` from the typed params and moves it
        // into `RequestContext::meta` before dispatch. Rehydrate the params so
        // task augmentation and progress-token handling below see the actual
        // wire metadata (typed params metadata remains useful for direct SDK
        // callers that bypass the transport loop).
        rehydrate_request_meta(&mut request, &context.meta);
        // SEP-2663: rmcp 3.x removed the dedicated `ServerHandler::enqueue_task`
        // hook and the typed `CallToolRequestParams::task` field that rmcp 1.x
        // used to route task-augmented `tools/call` requests. Task augmentation
        // is now opt-in through the request's `_meta` extension key, and the
        // server materializes the task by returning `CallToolResponse::Task`
        // from `call_tool` itself. `tasks::enqueue_task` runs its own authz
        // (`authorize_task_tool_call`), exactly as it did when the SDK routed
        // to it directly, so this branch stays ahead of the synchronous gate.
        if is_task_augmented(&request) {
            // The SDK rejects a `CallToolResponse::Task` outright when the
            // client never declared the tasks extension. Check first so a
            // capability-less client is refused *before* a job is enqueued
            // rather than after the side effect has already landed.
            if !context
                .client_capabilities()
                .is_some_and(|caps| caps.supports_tasks())
            {
                return Err(invalid_params(
                    "task-augmented tools/call requires the client to declare the \
                     `io.modelcontextprotocol/tasks` extension capability",
                ));
            }
            return tasks::enqueue_task(self, request, context)
                .await
                .map(Into::into);
        }

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
        server_authz::enforce_call_tool_scope(auth, request.name.as_ref(), &action, &subaction)?;

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

        // `memory`'s router-level scope gate only requires `axon:write`, but
        // `MemorySubaction::Import` with `mode: replace_scope` requires
        // `axon:admin` (`axon_api::source::MemoryImportMode::ReplaceScope`).
        // Resolve the real caller-derived authz here, never hardcoded;
        // `LoopbackDev` has no `AuthContext` and is locally-trusted, matching
        // the CLI's own local-trust rationale.
        let memory_authz = if action == "memory" {
            axon_services::memory::MemoryAuthz {
                is_admin: match auth {
                    None => true,
                    Some(auth_ctx) => {
                        axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE)
                    }
                },
            }
        } else {
            axon_services::memory::MemoryAuthz::anonymous()
        };

        // Real caller-derived AuthSnapshot for job-submission handlers
        // such as extract.start and future source-backed starts.
        // — `None` in LoopbackDev mode, where there is no per-caller identity
        // to snapshot and the loopback bind is the trust boundary itself.
        //
        // Visibility ceiling comes from `axon_authz::VisibilityPolicy`, not a
        // hardcoded `Internal` — mirrors `axon-web`'s
        // `caller_context_from_auth` (crates/axon-web/src/server/handlers/sources.rs).
        // A remote MCP caller is never `trusted_local`, so only callers who
        // additionally hold `axon:admin` get `Internal`; every other remote
        // caller is capped at `Public`, matching the identical REST caller.
        let caller_auth_snapshot = auth.map(|auth_ctx| {
            let auth_mode = if auth_ctx.sub == "static-bearer" {
                axon_api::source::AuthMode::StaticToken
            } else {
                axon_api::source::AuthMode::Oauth
            };
            let mut caller = axon_api::source::CallerContext {
                caller_id: Some(auth_ctx.sub.clone()),
                transport: axon_api::source::TransportKind::Mcp,
                trusted_local: false,
                scopes: auth_ctx.scopes.clone(),
                visibility_ceiling: axon_api::source::Visibility::Public,
                auth_mode,
                token_id: None,
                display_name: None,
            };
            let ceiling = axon_authz::VisibilityPolicy::new().ceiling_for(&caller);
            caller.visibility_ceiling = ceiling;
            axon_api::source::AuthSnapshot::from_caller(&caller, ceiling, "runtime")
        });

        // Delegate to the tool router generated by #[tool_router], with the
        // resolved prune/memory authz and caller auth snapshot available to
        // handlers via task-local (see `common.rs` module docs).
        let reset_authz = axon_services::reset::ResetAuthz {
            is_admin: action == "reset"
                && match auth {
                    None => true,
                    Some(auth_ctx) => {
                        axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE)
                    }
                },
        };
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        common::CURRENT_PRUNE_AUTHZ
            .scope(
                prune_authz,
                common::CURRENT_RESET_AUTHZ.scope(
                    reset_authz,
                    common::CURRENT_MEMORY_AUTHZ.scope(
                        memory_authz,
                        common::CURRENT_CALLER_AUTH_SNAPSHOT
                            .scope(caller_auth_snapshot, Self::tool_router().call(tcc)),
                    ),
                ),
            )
            .await
    }

    /// SEP-2663 `tasks/get`. Replaces rmcp 1.x's split `get_task_info`
    /// (`tasks/get`) + `get_task_result` (`tasks/result`) pair: the terminal
    /// result is now inlined in the returned [`rmcp::model::DetailedTask`]
    /// payload, so there is no separate result method to implement.
    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        tasks::get_task(self, request, context).await
    }

    /// SEP-2663 `tasks/update`. Axon's tasks are backed by durable jobs and
    /// never raise in-task server-to-client input requests, so there is
    /// nothing for a client to respond to.
    async fn update_task(
        &self,
        _request: UpdateTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        Err(invalid_params(
            "axon tasks never issue in-task input requests; tasks/update is not supported",
        ))
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
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
    ) -> Result<ReadResourceResponse, ErrorData> {
        handler_meta::read_resource(self, request, context)
            .await
            .map(Into::into)
    }
}

/// True when the caller opted this `tools/call` into SEP-2663 task
/// augmentation by carrying the tasks extension key in the request `_meta`.
///
/// rmcp 3.0.0-beta.2 does not surface this as a typed field (rmcp 1.x had
/// `CallToolRequestParams::task`), so the raw `_meta` key is the only
/// per-request signal available. Gating on the key — rather than on the
/// client's advertised capability alone — keeps the rmcp 1.x semantics where a
/// tasks-capable client still gets a synchronous result unless it explicitly
/// asked for a task.
fn is_task_augmented(request: &CallToolRequestParams) -> bool {
    request
        .meta
        .as_ref()
        .is_some_and(|meta| meta.contains_key(TASKS_EXTENSION_ID))
}

fn rehydrate_request_meta(request: &mut CallToolRequestParams, context_meta: &RequestMetaObject) {
    if request.meta.is_none() && !context_meta.is_empty() {
        request.meta = Some(context_meta.clone());
    }
}
