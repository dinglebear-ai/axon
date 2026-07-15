use std::error::Error;

use sqlx::SqlitePool;
use uuid::Uuid;

use axon_core::config::Config;
use axon_jobs::boundary::JobStore;
use axon_jobs::watch;

use crate::context::ServiceContext;

pub use axon_jobs::watch::{
    WatchDef, WatchDefCreate, WatchDefCreateRequest, WatchRun, WatchRunArtifact,
};

// Source-request-backed watch store (WS-B / audit C4-04, issue #298). This is
// a thin facade over `axon_jobs::watch_store::SqliteWatchStore` — the real
// `WatchStore` implementation — kept deliberately separate from the
// task_type/task_payload facade above (see `watch_store.rs` module docs for
// why the two models are not unified in this slice).
pub use axon_api::source::{
    AdapterOptions, AuthSnapshot, ExecutionMode, JobDescriptor, JobKind, SourceIntent,
    SourceRefreshPolicy, SourceRequest, SourceScope, SourceWatchPolicy, WatchExecRequest,
    WatchHistoryRequest, WatchHistoryResult, WatchId, WatchListRequest, WatchRequest, WatchResult,
    WatchSchedule, WatchSummary, WatchUpdateRequest,
};
pub use axon_jobs::boundary::WatchStore as SourceWatchStoreTrait;
pub use axon_jobs::watch_store::SqliteWatchStore;

/// Open a [`SqliteWatchStore`] against the given pool, or against a freshly
/// opened config-derived pool when no shared pool is available (mirrors the
/// `shared_pool`/`cfg` fallback pattern used by the watch facade below).
pub async fn open_source_watch_store(
    cfg: &Config,
    pool: Option<&SqlitePool>,
) -> Result<SqliteWatchStore, Box<dyn Error>> {
    let pool = match pool {
        Some(pool) => pool.clone(),
        None => axon_jobs::store::open_config_pool(cfg).await?,
    };
    Ok(SqliteWatchStore::new(pool))
}

/// Create a source-request-backed watch (issue #298 WS-B `POST /v1/watches`).
pub async fn create_source_watch(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    mut request: WatchRequest,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<WatchResult, Box<dyn Error>> {
    let effective_scope = authorize_watch_request(&request, auth_snapshot.as_ref())?;
    request.scope = Some(effective_scope);
    let store = open_source_watch_store(cfg, pool).await?;
    let stored_auth =
        Some(auth_snapshot.unwrap_or_else(|| AuthSnapshot::trusted_system("source-watch-create")));
    store
        .create_with_auth(request, stored_auth)
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)
}

pub async fn exec_source_watch(
    ctx: &ServiceContext,
    pool: Option<&SqlitePool>,
    watch_id: WatchId,
    request: WatchExecRequest,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<JobDescriptor, Box<dyn Error>> {
    let store = open_source_watch_store(ctx.cfg(), pool).await?;
    let (watch_request, stored_auth) = store
        .request_with_auth(watch_id.clone())
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?
        .ok_or_else(|| format!("watch {} not found", watch_id.0))?;
    authorize_watch_request(&watch_request, auth_snapshot.as_ref())?;
    let source_request = source_request_for_watch_exec(watch_request, &request);
    let run_auth = Some(stored_auth.unwrap_or_default());
    let job_store = ctx
        .job_store()
        .ok_or("watch exec requires a unified source job store")?;
    let result = enqueue_watch_source(source_request, job_store.as_ref(), run_auth).await?;
    let descriptor = result.job.ok_or_else(|| {
        let error = result
            .errors
            .first()
            .map(|error| error.message.clone())
            .or_else(|| {
                result
                    .warnings
                    .first()
                    .map(|warning| warning.message.clone())
            })
            .unwrap_or_else(|| "source job enqueue returned no job descriptor".to_string());
        format!("watch {} exec failed: {error}", watch_id.0)
    })?;
    SourceWatchStoreTrait::record_run(&store, watch_id, descriptor.job_id)
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    ctx.notify_unified();
    if request.wait.unwrap_or(false) {
        ctx.jobs
            .wait_for_job(descriptor.job_id.0, JobKind::Source)
            .await
            .map_err(|err| format!("watch exec wait failed: {err}"))?;
    }
    Ok(descriptor)
}

async fn enqueue_watch_source(
    request: SourceRequest,
    store: &dyn JobStore,
    auth_snapshot: Option<AuthSnapshot>,
) -> Result<axon_api::source::SourceResult, Box<dyn Error>> {
    crate::source::enqueue::enqueue_source(request, store, auth_snapshot)
        .await
        .map_err(|err| format!("watch exec enqueue failed: {err}").into())
}

fn authorize_watch_request(
    watch: &WatchRequest,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<SourceScope, Box<dyn Error>> {
    let source_request = source_request_for_watch_create(watch);
    let routed = crate::source::routing::resolve_source_route(&source_request)
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    crate::source::authorize::authorize_route(&routed.route)
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    crate::source::authorize::authorize_safety_class(routed.route.safety_class, auth_snapshot)
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    crate::source::security::authorize_local_source_policy(
        source_request.source.trim(),
        routed.kind,
        auth_snapshot,
    )
    .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    if routed.kind == crate::source::classify::SourceInputKind::Unsupported {
        return Err("watch source kind is unsupported".into());
    }
    Ok(routed.route.scope)
}

fn source_request_for_watch_create(watch: &WatchRequest) -> SourceRequest {
    let mut source = SourceRequest::new(watch.source.clone());
    source.intent = SourceIntent::Watch;
    source.watch = SourceWatchPolicy::Enabled;
    source.refresh = SourceRefreshPolicy::IfStale;
    source.embed = watch.embed;
    source.options = watch.options.clone();
    source.scope = watch.scope;
    source.collection = watch.collection.clone();
    source
}

fn source_request_for_watch_exec(watch: WatchRequest, request: &WatchExecRequest) -> SourceRequest {
    let mut source = SourceRequest::new(watch.source);
    source.intent = SourceIntent::Watch;
    source.watch = SourceWatchPolicy::Enabled;
    source.refresh = request.refresh.unwrap_or(SourceRefreshPolicy::IfStale);
    source.embed = watch.embed;
    source.options = watch.options;
    source.scope = watch.scope;
    source.collection = watch.collection;
    if request.wait.unwrap_or(false) {
        source.execution.mode = ExecutionMode::Wait;
    }
    if let Some(reason) = &request.reason {
        source
            .metadata
            .insert("watch_exec_reason".to_string(), serde_json::json!(reason));
    }
    source
}

pub async fn history_source_watch(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    request: WatchHistoryRequest,
) -> Result<WatchHistoryResult, Box<dyn Error>> {
    let store = open_source_watch_store(cfg, pool).await?;
    SourceWatchStoreTrait::history(&store, request)
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)
}

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch::list_watch_defs(cfg, limit).await
}

pub async fn list_watch_defs_with_pool(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch::list_watch_defs_with_pool(pool, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch::create_watch_def(cfg, input).await
}

pub async fn create_watch_def_with_pool(
    pool: &SqlitePool,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch::create_watch_def_with_pool(pool, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch::list_watch_runs(cfg, watch_id, limit).await
}

pub async fn list_watch_runs_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch::list_watch_runs_with_pool(pool, watch_id, limit).await
}

pub async fn list_watch_run_artifacts(
    cfg: &Config,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    watch::list_watch_run_artifacts(cfg, run_id, limit).await
}

pub async fn list_watch_run_artifacts_with_pool(
    pool: &SqlitePool,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    watch::list_watch_run_artifacts_with_pool(pool, run_id, limit).await
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    watch::create_watch_run(cfg, watch_id, dispatched_job_id).await
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def(cfg, watch_id).await
}

pub async fn get_watch_def_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def_with_pool(pool, watch_id).await
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    watch::finish_watch_run(cfg, watch_id, run_id, status, result_json, error_text).await
}

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    watch::run_watch_now(cfg, watch).await
}

pub async fn run_watch_now_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
    watch: &WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    watch::run_watch_now_with_pool(cfg, pool, watch).await
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
