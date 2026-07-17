//! `/v1/watches` — the canonical source-request-backed watch surface
//! (issue #298 REST contract, `docs/pipeline-unification/surfaces/rest-contract.md`
//! "Watch Routes" table).
//!
//! `/v1/watches` is backed by [`axon_services::watch::SqliteWatchStore`]
//! (`axon_source_watches`/`axon_source_watch_runs`, migration `0023`), matching
//! `axon_api::source::{WatchRequest, WatchResult}`. `/v1/watches` covers
//! create/list/get/status/update/pause/resume/delete/exec/history.
//!
//! `POST /v1/watches/{watch_id}/exec` (issue #298 REST contract) is the
//! canonical replacement for the removed `POST /v1/watch/{id}/run`; it enqueues
//! a source job and records that job in canonical watch history.

use axon_api::source::{
    AuthMode, AuthSnapshot, CallerContext, LifecycleStatus, TransportKind, Visibility,
    WatchExecRequest, WatchHistoryRequest, WatchId, WatchListRequest, WatchRequest,
    WatchUpdateRequest,
};
use axon_authz::VisibilityPolicy;
use axon_core::config::Config;
use axon_services::watch as watch_svc;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use lab_auth::AuthContext;
use serde::Deserialize;
use serde_json::json;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, std::sync::Arc<Config>);

async fn open_store(
    state: &super::super::state::AppState,
    cfg: &Config,
) -> Result<watch_svc::SqliteWatchStore, HttpError> {
    let pool = state.service_context.jobs.sqlite_pool();
    watch_svc::open_source_watch_store(cfg, pool.as_deref())
        .await
        .map_err(HttpError::from_box)
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub(crate) struct WatchListQuery {
    enabled: Option<bool>,
    source_id: Option<String>,
    adapter: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub(crate) struct WatchHistoryQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    status: Option<LifecycleStatus>,
}

#[utoipa::path(
    post,
    path = "/v1/watches",
    operation_id = "watches_create",
    request_body = WatchRequest,
    responses(
        (status = 200, description = "Created watch detail", body = serde_json::Value),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn create_watch(
    State((state, cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<WatchRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let pool = state.service_context.jobs.sqlite_pool();
    let auth_snapshot = auth
        .as_ref()
        .map(|Extension(auth)| auth_snapshot_from_context(auth));
    let created = watch_svc::create_source_watch(&cfg, pool.as_deref(), request, auth_snapshot)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!(created)))
}

#[utoipa::path(
    get,
    path = "/v1/watches",
    operation_id = "watches_list",
    params(WatchListQuery),
    responses(
        (status = 200, description = "Paged watch summaries", body = serde_json::Value),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn list_watches(
    State((state, cfg)): State<WebState>,
    Query(query): Query<WatchListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let request = WatchListRequest {
        enabled: query.enabled,
        source_id: query.source_id.map(axon_api::source::SourceId::new),
        adapter: query.adapter,
        limit: query.limit,
        cursor: query.cursor,
    };
    let page = watch_svc::SourceWatchStoreTrait::list(&store, request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(page)))
}

#[utoipa::path(
    get,
    path = "/v1/watches/{watch_id}",
    operation_id = "watches_get",
    params(("watch_id" = String, Path, description = "Watch ID")),
    responses(
        (status = 200, description = "Watch detail", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn get_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    match watch_svc::SourceWatchStoreTrait::get(&store, WatchId::new(watch_id.clone()))
        .await
        .map_err(HttpError::from_api_error)?
    {
        Some(watch) => Ok(Json(json!(watch))),
        None => Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("watch {watch_id} not found"),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/v1/watches/{watch_id}/status",
    operation_id = "watches_status",
    params(("watch_id" = String, Path, description = "Watch ID")),
    responses(
        (status = 200, description = "Watch status with latest source job summary", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch status unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn status_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let watch_id_typed = WatchId::new(watch_id.clone());
    let store = open_store(&state, &cfg).await?;
    let watch = watch_svc::SourceWatchStoreTrait::get(&store, watch_id_typed.clone())
        .await
        .map_err(HttpError::from_api_error)?
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("watch {watch_id} not found"),
            )
        })?;
    let latest_job_summary = match watch.latest_job.as_ref() {
        Some(job) => axon_services::jobs::unified_job_status(&state.service_context, job.job_id)
            .await
            .map_err(HttpError::from_box_send_sync)?,
        None => None,
    };
    Ok(Json(json!({
        "watch": watch,
        "latest_job_summary": latest_job_summary,
    })))
}

#[utoipa::path(
    post,
    path = "/v1/watches/{watch_id}/exec",
    operation_id = "watches_exec",
    params(("watch_id" = String, Path, description = "Watch ID")),
    request_body = WatchExecRequest,
    responses(
        (status = 200, description = "Watch execution job descriptor", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch execution failed", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn exec_watch(
    State((state, cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Path(watch_id): Path<String>,
    Json(request): Json<WatchExecRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let watch_id_typed = WatchId::new(watch_id.clone());
    // 404 up front against the canonical store so unknown source watch ids get
    // a clean not-found response before enqueueing any source work.
    let store = open_store(&state, &cfg).await?;
    if watch_svc::SourceWatchStoreTrait::get(&store, watch_id_typed.clone())
        .await
        .map_err(HttpError::from_api_error)?
        .is_none()
    {
        return Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("watch {watch_id} not found"),
        ));
    }

    let pool = state.service_context.jobs.sqlite_pool();
    let auth_snapshot = auth
        .as_ref()
        .map(|Extension(auth)| auth_snapshot_from_context(auth));
    let descriptor = watch_svc::exec_source_watch(
        &state.service_context,
        pool.as_deref(),
        watch_id_typed,
        request,
        auth_snapshot,
    )
    .await
    .map_err(HttpError::from_box)?;
    Ok(Json(json!(descriptor)))
}

#[utoipa::path(
    get,
    path = "/v1/watches/{watch_id}/history",
    operation_id = "watches_history",
    params(("watch_id" = String, Path, description = "Watch ID"), WatchHistoryQuery),
    responses(
        (status = 200, description = "Watch execution history", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch history lookup failed", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn history_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
    Query(query): Query<WatchHistoryQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let watch_id_typed = WatchId::new(watch_id.clone());
    let store = open_store(&state, &cfg).await?;
    if watch_svc::SourceWatchStoreTrait::get(&store, watch_id_typed.clone())
        .await
        .map_err(HttpError::from_api_error)?
        .is_none()
    {
        return Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("watch {watch_id} not found"),
        ));
    }

    let pool = state.service_context.jobs.sqlite_pool();
    let history = watch_svc::history_source_watch(
        &cfg,
        pool.as_deref(),
        watch_history_request(watch_id_typed, query),
    )
    .await
    .map_err(HttpError::from_box)?;
    Ok(Json(json!(history)))
}

fn watch_history_request(watch_id: WatchId, query: WatchHistoryQuery) -> WatchHistoryRequest {
    WatchHistoryRequest {
        watch_id,
        limit: query.limit,
        cursor: query.cursor,
        status: query.status,
    }
}

fn auth_snapshot_from_context(auth: &AuthContext) -> AuthSnapshot {
    AuthSnapshot::from_caller(
        &caller_context_from_auth(auth),
        Visibility::Internal,
        "runtime",
    )
}

fn caller_context_from_auth(auth: &AuthContext) -> CallerContext {
    let auth_mode = if auth.sub == "static-bearer" {
        AuthMode::StaticToken
    } else {
        AuthMode::Oauth
    };
    let mut caller = CallerContext {
        caller_id: Some(auth.sub.clone()),
        transport: TransportKind::Rest,
        trusted_local: false,
        scopes: auth.scopes.clone(),
        visibility_ceiling: Visibility::Public,
        auth_mode,
        token_id: None,
        display_name: None,
    };
    caller.visibility_ceiling = VisibilityPolicy::new().ceiling_for(&caller);
    caller
}

#[utoipa::path(
    patch,
    path = "/v1/watches/{watch_id}",
    operation_id = "watches_update",
    params(("watch_id" = String, Path, description = "Watch ID")),
    request_body = WatchUpdateRequest,
    responses(
        (status = 200, description = "Updated watch detail", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn update_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
    Json(request): Json<WatchUpdateRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let updated = watch_svc::SourceWatchStoreTrait::update(&store, WatchId::new(watch_id), request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(updated)))
}

#[utoipa::path(
    post,
    path = "/v1/watches/{watch_id}/pause",
    operation_id = "watches_pause",
    params(("watch_id" = String, Path, description = "Watch ID")),
    responses(
        (status = 200, description = "Updated watch detail", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn pause_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let request = WatchUpdateRequest {
        enabled: Some(false),
        schedule: None,
        options: None,
        embed: None,
        collection: None,
        scope: None,
    };
    let updated = watch_svc::SourceWatchStoreTrait::update(&store, WatchId::new(watch_id), request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(updated)))
}

#[utoipa::path(
    post,
    path = "/v1/watches/{watch_id}/resume",
    operation_id = "watches_resume",
    params(("watch_id" = String, Path, description = "Watch ID")),
    responses(
        (status = 200, description = "Updated watch detail", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn resume_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let request = WatchUpdateRequest {
        enabled: Some(true),
        schedule: None,
        options: None,
        embed: None,
        collection: None,
        scope: None,
    };
    let updated = watch_svc::SourceWatchStoreTrait::update(&store, WatchId::new(watch_id), request)
        .await
        .map_err(HttpError::from_api_error)?;
    Ok(Json(json!(updated)))
}

#[utoipa::path(
    delete,
    path = "/v1/watches/{watch_id}",
    operation_id = "watches_delete",
    params(("watch_id" = String, Path, description = "Watch ID")),
    responses(
        (status = 200, description = "Deletion result", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn delete_watch(
    State((state, cfg)): State<WebState>,
    Path(watch_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let store = open_store(&state, &cfg).await?;
    let deleted = store
        .delete(WatchId::new(watch_id.clone()))
        .await
        .map_err(HttpError::from_api_error)?;
    if !deleted {
        return Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("watch {watch_id} not found"),
        ));
    }
    Ok(Json(json!({ "watch_id": watch_id, "deleted": true })))
}

#[cfg(test)]
#[path = "source_watch_tests.rs"]
mod tests;
