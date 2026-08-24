use super::AxonMcpServer;
use super::common::{invalid_params, logged_internal_error, validate_mcp_urls};
use super::server_authz;
use super::task_id::{parse_task_id, task_id_for};
use super::task_progress;
use super::task_status::{detailed_task_from_job, task_from_job, task_meta_from_job};
use crate::schema::{AxonRequest, ExtractSubaction, parse_axon_request};
use axon_api::source::JobKind;
use axon_core::config::ConfigOverrides;
use axon_services::extract as extract_svc;
use axon_services::types::ServiceJob;
use rmcp::model::{
    CallToolRequestParams, CancelTaskParams, CreateTaskResult, GetTaskParams, GetTaskResult,
};
use rmcp::{ErrorData, RoleServer, service::RequestContext};
use serde_json::{Map, Value};
use uuid::Uuid;

pub(super) async fn enqueue_task(
    server: &AxonMcpServer,
    request: CallToolRequestParams,
    context: RequestContext<RoleServer>,
) -> Result<CreateTaskResult, ErrorData> {
    if request.name.as_ref() != "axon" {
        return Err(invalid_params(format!(
            "tool `{}` does not support task execution",
            request.name
        )));
    }

    let progress_token = request
        .meta
        .as_ref()
        .and_then(|meta| meta.get_progress_token());
    let raw = request
        .arguments
        .clone()
        .ok_or_else(|| invalid_params("arguments are required for task execution"))?;
    let axon_request =
        parse_axon_request(raw).map_err(|e| invalid_params(format!("invalid request: {e}")))?;
    let auth = authorize_task_tool_call(server, &request, &context)?;
    let caller_auth_snapshot = auth.map(caller_auth_snapshot_from_auth_context);
    let (kind, job_id) =
        enqueue_supported_start(server, axon_request, caller_auth_snapshot.as_ref()).await?;
    task_progress::start_progress_notifier(
        server,
        kind,
        job_id,
        progress_token,
        context.peer.clone(),
    )
    .await;
    let job = load_job(server, kind, job_id).await?;
    Ok(CreateTaskResult::new(task_from_job(kind, &job)))
}

/// SEP-2663 `tasks/get`.
///
/// Subsumes rmcp 1.x's `tasks/get` + `tasks/result` pair: `DetailedTask`
/// inlines the terminal payload, so a completed job's result is returned here
/// instead of from a separate blocking `tasks/result` call.
pub(super) async fn get_task(
    server: &AxonMcpServer,
    request: GetTaskParams,
    context: RequestContext<RoleServer>,
) -> Result<GetTaskResult, ErrorData> {
    authorize_task_lifecycle(server, &context, "tasks/get")?;
    let (kind, job_id) = parse_task_id(&request.task_id)?;
    let job = load_job(server, kind, job_id).await?;
    let mut result = GetTaskResult::new(detailed_task_from_job(kind, &job));
    result.meta = task_meta_from_job(kind, &job);
    Ok(result)
}

/// SEP-2663 `tasks/cancel`. The ack is empty now — the post-cancel task state
/// is observed through the next `tasks/get` rather than returned inline.
pub(super) async fn cancel_task(
    server: &AxonMcpServer,
    request: CancelTaskParams,
    context: RequestContext<RoleServer>,
) -> Result<(), ErrorData> {
    authorize_task_lifecycle(server, &context, "tasks/cancel")?;
    let (kind, job_id) = parse_task_id(&request.task_id)?;
    let ctx = server
        .base_service_context()
        .await
        .map_err(|e| logged_internal_error("tasks.cancel.context", e.as_ref()))?;
    let canceled = ctx
        .jobs
        .cancel_job(kind, job_id)
        .await
        .map_err(|e| logged_internal_error("tasks.cancel", e.as_ref()))?;
    if !canceled {
        return Err(invalid_params(format!(
            "task is not active and cannot be cancelled: {}",
            task_id_for(kind, job_id)
        )));
    }
    Ok(())
}

/// Enforces scope for the in-flight task tool call and returns the resolved
/// caller [`lab_auth::AuthContext`] (`None` in LoopbackDev mode) so callers
/// can build a real [`axon_api::source::AuthSnapshot`] for job submission
/// instead of falling back to `trusted_system`.
fn authorize_task_tool_call<'a>(
    server: &AxonMcpServer,
    request: &CallToolRequestParams,
    context: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a lab_auth::AuthContext>, ErrorData> {
    let auth = server_authz::require_auth_context(&server.auth_policy, context)?;
    let (action, subaction) = action_pair_from_arguments(request.arguments.as_ref());
    // mutates_if (axon #298 follow-up): mirrors the upgrade applied in
    // `server.rs::call_tool` so the deferred-task path enforces the same
    // effective scope as the synchronous dispatch path.
    let base_required_scope = server_authz::required_scope_for_tool("axon", &action, &subaction);
    let required_scope = server_authz::required_scope_with_mutates_if(&action, base_required_scope);
    // CWE-863 fix: mirrors the `is_elevated` branch in `server.rs::call_tool`
    // — when `mutates_if_upgrade` elevated the requirement, this deferred
    // task-tool path must use the same strict `check_scope_explicit`, not
    // the broad `check_scope`, or the elevation is a silent no-op here too.
    let is_elevated = server_authz::mutates_if_upgrade(&action).is_some();
    match (auth, required_scope) {
        (Some(_), Some("__deny__")) => Err(ErrorData::invalid_request(
            format!("forbidden: unknown action `{action}`"),
            None,
        )),
        (Some(auth_ctx), None) => Ok(Some(auth_ctx)),
        (Some(auth_ctx), Some(required_scope)) if is_elevated => {
            server_authz::check_scope_explicit(auth_ctx, required_scope, &action)?;
            Ok(Some(auth_ctx))
        }
        (Some(auth_ctx), Some(required_scope)) => {
            server_authz::check_scope(auth_ctx, required_scope, &action)?;
            Ok(Some(auth_ctx))
        }
        (None, _) => Ok(None),
    }
}

/// Build an [`axon_api::source::AuthSnapshot`] from a resolved MCP
/// [`lab_auth::AuthContext`] — mirrors `server.rs::call_tool`'s
/// `caller_auth_snapshot` construction for the `enqueue_task` (rmcp Tasks)
/// path, which receives its own `RequestContext` directly instead of going
/// through the `CURRENT_CALLER_AUTH_SNAPSHOT` task-local.
///
/// Visibility ceiling comes from `axon_authz::VisibilityPolicy`, not a
/// hardcoded `Internal` — mirrors `axon-web`'s `caller_context_from_auth`
/// (crates/axon-web/src/server/handlers/sources.rs). A remote MCP caller is
/// never `trusted_local`, so only callers who additionally hold `axon:admin`
/// get `Internal`; every other remote caller is capped at `Public`, matching
/// the identical REST caller.
fn caller_auth_snapshot_from_auth_context(
    auth_ctx: &lab_auth::AuthContext,
) -> axon_api::source::AuthSnapshot {
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
}

fn authorize_task_lifecycle(
    server: &AxonMcpServer,
    context: &RequestContext<RoleServer>,
    action: &str,
) -> Result<(), ErrorData> {
    let auth = server_authz::require_auth_context(&server.auth_policy, context)?;
    if let Some(auth_ctx) = auth {
        server_authz::check_scope(auth_ctx, "axon:write", action)?;
    }
    Ok(())
}

fn action_pair_from_arguments(arguments: Option<&Map<String, Value>>) -> (String, String) {
    let action = arguments
        .and_then(|args| args.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let subaction = arguments
        .and_then(|args| args.get("subaction"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    (action, subaction)
}

async fn enqueue_supported_start(
    server: &AxonMcpServer,
    request: AxonRequest,
    caller_auth_snapshot: Option<&axon_api::source::AuthSnapshot>,
) -> Result<(JobKind, Uuid), ErrorData> {
    match request {
        AxonRequest::Extract(req)
            if matches!(req.subaction, None | Some(ExtractSubaction::Start)) =>
        {
            let urls = req
                .urls
                .ok_or_else(|| invalid_params("urls is required for extract.start"))?;
            if urls.is_empty() {
                return Err(invalid_params("urls cannot be empty"));
            }
            validate_mcp_urls(&urls)?;
            let cfg = server.cfg.apply_overrides(&ConfigOverrides {
                query: Some(req.prompt),
                max_pages: req.max_pages,
                wait: Some(false),
                ..ConfigOverrides::default()
            });
            let service_context = server
                .base_service_context()
                .await
                .map_err(|e| logged_internal_error("tasks.extract.start.context", e.as_ref()))?;
            // Real caller-derived AuthSnapshot from `enqueue_task`'s own
            // `RequestContext` (see `authorize_task_tool_call` /
            // `caller_auth_snapshot_from_auth_context` above). `None` only in
            // LoopbackDev mode, where `extract_start_with_context` falls back
            // to `trusted_system`, same as before.
            let outcome = extract_svc::extract_start_with_context(
                &cfg,
                &urls,
                cfg.query.clone(),
                &service_context,
                None,
                caller_auth_snapshot,
            )
            .await
            .map_err(|e| logged_internal_error("tasks.extract.start", e.as_ref()))?;
            Ok((JobKind::Extract, parse_uuid(&outcome.result.job_id)?))
        }
        other => Err(unsupported_task_request(&other)),
    }
}

async fn load_job(
    server: &AxonMcpServer,
    kind: JobKind,
    job_id: Uuid,
) -> Result<ServiceJob, ErrorData> {
    let ctx = server
        .base_service_context()
        .await
        .map_err(|e| logged_internal_error("tasks.status.context", e.as_ref()))?;
    ctx.jobs
        .job_status(kind, job_id)
        .await
        .map_err(|e| logged_internal_error("tasks.status", e.as_ref()))?
        .ok_or_else(|| invalid_params(format!("task not found: {}", task_id_for(kind, job_id))))
}

fn parse_uuid(raw: &str) -> Result<Uuid, ErrorData> {
    Uuid::parse_str(raw)
        .map_err(|e| ErrorData::internal_error(format!("invalid queued job id: {e}"), None))
}

fn unsupported_task_request(request: &AxonRequest) -> ErrorData {
    let (action, subaction) = match request {
        AxonRequest::Scrape(_) => ("scrape", "None".to_string()),
        AxonRequest::Crawl(_) => ("crawl", "None".to_string()),
        AxonRequest::Embed(_) => ("embed", "None".to_string()),
        AxonRequest::Ingest(_) => ("ingest", "None".to_string()),
        AxonRequest::CodeSearch(_) => ("code_search", "None".to_string()),
        AxonRequest::Extract(req) => ("extract", format!("{:?}", req.subaction)),
        AxonRequest::Memory(req) => ("memory", format!("{:?}", req.subaction)),
        AxonRequest::Status(_) => ("status", "None".to_string()),
        AxonRequest::Jobs(req) => ("jobs", format!("{:?}", req.subaction)),
        AxonRequest::Help(_) => ("help", "None".to_string()),
        AxonRequest::Query(_) => ("query", "None".to_string()),
        AxonRequest::Retrieve(_) => ("retrieve", "None".to_string()),
        AxonRequest::Search(_) => ("search", "None".to_string()),
        AxonRequest::Map(_) => ("map", "None".to_string()),
        AxonRequest::Endpoints(_) => ("endpoints", "None".to_string()),
        AxonRequest::Evaluate(_) => ("evaluate", "None".to_string()),
        AxonRequest::Suggest(_) => ("suggest", "None".to_string()),
        AxonRequest::Doctor(_) => ("doctor", "None".to_string()),
        AxonRequest::Domains(_) => ("domains", "None".to_string()),
        AxonRequest::Sources(_) => ("sources", "None".to_string()),
        AxonRequest::Stats(_) => ("stats", "None".to_string()),
        AxonRequest::Source(_) => ("source", "None".to_string()),
        AxonRequest::Research(_) => ("research", "None".to_string()),
        AxonRequest::Ask(_) => ("ask", "None".to_string()),
        AxonRequest::Summarize(_) => ("summarize", "None".to_string()),
        AxonRequest::Screenshot(_) => ("screenshot", "None".to_string()),
        AxonRequest::Brand(_) => ("brand", "None".to_string()),
        AxonRequest::Diff(_) => ("diff", "None".to_string()),
        AxonRequest::Debug(_) => ("debug", "None".to_string()),
        AxonRequest::Prune(_) => ("prune", "None".to_string()),
        AxonRequest::Migrate(_) => ("migrate", "None".to_string()),
        AxonRequest::Watch(_) => ("watch", "None".to_string()),
        AxonRequest::Setup(_) => ("setup", "None".to_string()),
        AxonRequest::Resolve(_) => ("resolve", "None".to_string()),
        AxonRequest::Capabilities(_) => ("capabilities", "None".to_string()),
        AxonRequest::Providers(req) => ("providers", format!("{:?}", req.subaction)),
        AxonRequest::Graph(req) => ("graph", format!("{:?}", req.subaction)),
        AxonRequest::Chat(_) => ("chat", "None".to_string()),
    };
    invalid_params(format!(
        "task execution is supported only for extract.start; got {action}.{subaction}"
    ))
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
