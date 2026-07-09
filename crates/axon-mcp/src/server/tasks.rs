use super::AxonMcpServer;
use super::common::{invalid_params, logged_internal_error, validate_mcp_urls};
use super::server_authz;
use super::task_id::{parse_task_id, task_id_for};
use super::task_progress;
use super::task_status::{task_from_job, task_result_payload};
use crate::schema::{AxonRequest, ExtractSubaction, parse_axon_request};
use axon_core::config::{ConfigOverrides, parse::tuning};
use axon_jobs::backend::JobKind;
use axon_services::extract as extract_svc;
use axon_services::types::ServiceJob;
use rmcp::model::{
    CallToolRequestParams, CancelTaskParams, CancelTaskResult, CreateTaskResult, GetTaskInfoParams,
    GetTaskPayloadResult, GetTaskResult, GetTaskResultParams, ListTasksResult,
    PaginatedRequestParams,
};
use rmcp::{ErrorData, RoleServer, service::RequestContext};
use serde_json::{Map, Value};
use uuid::Uuid;

const TASK_LIST_LIMIT: usize = 20;
const TASK_LIST_MAX_OFFSET: usize = 200;

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

pub(super) async fn list_tasks(
    server: &AxonMcpServer,
    request: Option<PaginatedRequestParams>,
    context: RequestContext<RoleServer>,
) -> Result<ListTasksResult, ErrorData> {
    authorize_task_lifecycle(server, &context, "tasks/list")?;
    let offset = parse_cursor_offset(request.and_then(|params| params.cursor))?;
    let fetch_limit = offset + TASK_LIST_LIMIT + 1;
    let ctx = server
        .base_service_context()
        .await
        .map_err(|e| logged_internal_error("tasks.list.context", e.as_ref()))?;
    let mut tasks = Vec::new();
    for kind in JobKind::all() {
        let jobs = ctx
            .jobs
            .list_jobs(*kind, fetch_limit as i64, 0)
            .await
            .map_err(|e| logged_internal_error("tasks.list", e.as_ref()))?;
        tasks.extend(jobs.into_iter().map(|job| (*kind, job)));
    }
    tasks.sort_by(|(_, left), (_, right)| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| right.id.cmp(&left.id))
    });

    let next_offset = offset + TASK_LIST_LIMIT;
    let next_cursor = (tasks.len() > next_offset && next_offset <= TASK_LIST_MAX_OFFSET)
        .then(|| next_offset.to_string());
    let page = tasks
        .into_iter()
        .skip(offset)
        .take(TASK_LIST_LIMIT)
        .map(|(kind, job)| task_from_job(kind, &job))
        .collect();
    let mut result = ListTasksResult::new(page);
    result.next_cursor = next_cursor;
    Ok(result)
}

pub(super) async fn get_task_info(
    server: &AxonMcpServer,
    request: GetTaskInfoParams,
    context: RequestContext<RoleServer>,
) -> Result<GetTaskResult, ErrorData> {
    authorize_task_lifecycle(server, &context, "tasks/get")?;
    let (kind, job_id) = parse_task_id(&request.task_id)?;
    let job = load_job(server, kind, job_id).await?;
    Ok(GetTaskResult {
        meta: None,
        task: task_from_job(kind, &job),
    })
}

pub(super) async fn get_task_result(
    server: &AxonMcpServer,
    request: GetTaskResultParams,
    context: RequestContext<RoleServer>,
) -> Result<GetTaskPayloadResult, ErrorData> {
    authorize_task_lifecycle(server, &context, "tasks/result")?;
    let (kind, job_id) = parse_task_id(&request.task_id)?;
    let job = tokio::time::timeout(
        task_result_wait_timeout(),
        wait_for_terminal_job(server, kind, job_id),
    )
    .await
    .map_err(|_| {
        invalid_params(format!(
            "task result timed out before terminal state: {}",
            task_id_for(kind, job_id)
        ))
    })??;
    Ok(task_result_payload(kind, &job))
}

pub(super) async fn cancel_task(
    server: &AxonMcpServer,
    request: CancelTaskParams,
    context: RequestContext<RoleServer>,
) -> Result<CancelTaskResult, ErrorData> {
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
    let job = load_job(server, kind, job_id).await?;
    Ok(CancelTaskResult {
        meta: None,
        task: task_from_job(kind, &job),
    })
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
    match (
        auth,
        server_authz::required_scope_for_tool("axon", &action, &subaction),
    ) {
        (Some(_), Some("__deny__")) => Err(ErrorData::invalid_request(
            format!("forbidden: unknown action `{action}`"),
            None,
        )),
        (Some(auth_ctx), None) => Ok(Some(auth_ctx)),
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
fn caller_auth_snapshot_from_auth_context(
    auth_ctx: &lab_auth::AuthContext,
) -> axon_api::source::AuthSnapshot {
    axon_api::source::AuthSnapshot::from_caller(
        &axon_api::source::CallerContext {
            actor: Some(auth_ctx.sub.clone()),
            transport: axon_api::source::TransportKind::Mcp,
            scopes: auth_ctx.scopes.clone(),
            visibility_ceiling: axon_api::source::Visibility::Internal,
        },
        axon_api::source::Visibility::Internal,
        "runtime",
    )
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

async fn wait_for_terminal_job(
    server: &AxonMcpServer,
    kind: JobKind,
    job_id: Uuid,
) -> Result<ServiceJob, ErrorData> {
    loop {
        let job = load_job(server, kind, job_id).await?;
        if !job.status_enum().is_active() {
            return Ok(job);
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            super::task_status::TASK_POLL_INTERVAL_MS,
        ))
        .await;
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, ErrorData> {
    Uuid::parse_str(raw)
        .map_err(|e| ErrorData::internal_error(format!("invalid queued job id: {e}"), None))
}

fn task_result_wait_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(tuning::mcp_task_result_wait_timeout_secs())
}

fn parse_cursor_offset(cursor: Option<String>) -> Result<usize, ErrorData> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let offset = cursor
        .parse::<usize>()
        .map_err(|_| invalid_params("tasks/list cursor must be a numeric offset"))?;
    if offset > TASK_LIST_MAX_OFFSET {
        return Err(invalid_params(format!(
            "tasks/list cursor must be <= {TASK_LIST_MAX_OFFSET}"
        )));
    }
    Ok(offset)
}

fn unsupported_task_request(request: &AxonRequest) -> ErrorData {
    let (action, subaction) = match request {
        AxonRequest::Crawl(req) => ("crawl", format!("{:?}", req.subaction)),
        AxonRequest::Extract(req) => ("extract", format!("{:?}", req.subaction)),
        AxonRequest::Embed(req) => ("embed", format!("{:?}", req.subaction)),
        AxonRequest::Ingest(req) => ("ingest", format!("{:?}", req.subaction)),
        AxonRequest::Memory(req) => ("memory", format!("{:?}", req.subaction)),
        AxonRequest::Status(_) => ("status", "None".to_string()),
        AxonRequest::Jobs(req) => ("jobs", format!("{:?}", req.subaction)),
        AxonRequest::Help(_) => ("help", "None".to_string()),
        AxonRequest::Query(_) => ("query", "None".to_string()),
        AxonRequest::CodeSearch(_) => ("code_search", "None".to_string()),
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
        AxonRequest::Scrape(_) => ("scrape", "None".to_string()),
        AxonRequest::VerticalScrape(_) => ("vertical_scrape", "None".to_string()),
        AxonRequest::Source(_) => ("source", "None".to_string()),
        AxonRequest::Research(_) => ("research", "None".to_string()),
        AxonRequest::Ask(_) => ("ask", "None".to_string()),
        AxonRequest::Summarize(_) => ("summarize", "None".to_string()),
        AxonRequest::Screenshot(_) => ("screenshot", "None".to_string()),
        AxonRequest::ElicitDemo(_) => ("elicit_demo", "None".to_string()),
        AxonRequest::Brand(_) => ("brand", "None".to_string()),
        AxonRequest::Diff(_) => ("diff", "None".to_string()),
        AxonRequest::Debug(_) => ("debug", "None".to_string()),
        AxonRequest::Dedupe(_) => ("dedupe", "None".to_string()),
        AxonRequest::Purge(_) => ("purge", "None".to_string()),
        AxonRequest::Prune(_) => ("prune", "None".to_string()),
        AxonRequest::Migrate(_) => ("migrate", "None".to_string()),
        AxonRequest::Watch(_) => ("watch", "None".to_string()),
        AxonRequest::Setup(_) => ("setup", "None".to_string()),
    };
    invalid_params(format!(
        "task execution is supported only for extract.start; got {action}.{subaction}"
    ))
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
