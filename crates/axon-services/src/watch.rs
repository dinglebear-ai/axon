use std::error::Error;

use sqlx::SqlitePool;

use axon_core::config::Config;
use axon_jobs::boundary::JobStore;

use crate::context::ServiceContext;

// Source-request-backed watch store (WS-B / audit C4-04, issue #298). This is
// a thin facade over `axon_jobs::watch_store::SqliteWatchStore` — the real
// `WatchStore` implementation.
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
    request.source = request.source.trim().to_string();
    let routed = authorize_watch_request(&request, auth_snapshot.as_ref())?;
    request.scope = Some(routed.route.scope);
    let store = open_source_watch_store(cfg, pool).await?;
    let mut existing = store
        .find_by_source(&routed.route.source.canonical_uri)
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    if existing.is_none() {
        existing = store
            .find_by_source(&request.source)
            .await
            .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    }
    if let Some(existing) = existing {
        let updated = SourceWatchStoreTrait::update(
            &store,
            existing.watch_id,
            WatchUpdateRequest {
                enabled: Some(request.enabled.unwrap_or(true)),
                schedule: Some(request.schedule),
                options: Some(request.options),
                embed: Some(request.embed),
                collection: request.collection,
                scope: Some(routed.route.scope),
            },
        )
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
        return Ok(updated);
    }
    let stored_auth =
        Some(auth_snapshot.unwrap_or_else(|| AuthSnapshot::trusted_system("source-watch-create")));
    store
        .create_resolved_with_auth(
            request,
            routed.route.source.source_id,
            routed.route.source.canonical_uri,
            routed.route.adapter,
            stored_auth,
        )
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

pub async fn resolve_source_watch_id(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    id_or_source: &str,
) -> Result<WatchId, Box<dyn Error>> {
    let store = open_source_watch_store(cfg, pool).await?;
    let watch_id = WatchId::new(id_or_source.trim());
    if SourceWatchStoreTrait::get(&store, watch_id.clone())
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?
        .is_some()
    {
        return Ok(watch_id);
    }
    let mut found = store
        .find_by_source(id_or_source)
        .await
        .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    if found.is_none()
        && let Some(canonical_uri) = source_canonical_uri(id_or_source)
    {
        found = store
            .find_by_source(&canonical_uri)
            .await
            .map_err(|err| Box::new(err) as Box<dyn Error>)?;
    }
    let Some(found) = found else {
        return Err(format!("watch {id_or_source} not found").into());
    };
    Ok(found.watch_id)
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
) -> Result<crate::source::routing::RoutedSource, Box<dyn Error>> {
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
    Ok(routed)
}

fn source_canonical_uri(source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    let request = SourceRequest::new(source.to_string());
    crate::source::routing::resolve_source_route(&request)
        .ok()
        .map(|routed| routed.route.source.canonical_uri)
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

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
