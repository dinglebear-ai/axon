//! `/v1/watches` — the canonical source-request-backed watch surface
//! (issue #298 REST contract, `docs/pipeline-unification/surfaces/rest-contract.md`
//! "Watch Routes" table).
//!
//! This is a distinct model from the legacy `/v1/watch` task_type/task_payload
//! surface in [`super::admin`] (`axon_watch_defs`/`axon_watch_runs`, migration
//! `0002`). `/v1/watches` is backed by [`axon_services::watch::SqliteWatchStore`]
//! (`axon_source_watches`/`axon_source_watch_runs`, migration `0023`), matching
//! `axon_api::source::{WatchRequest, WatchResult}`. See
//! `crates/axon-jobs/src/watch_store.rs` module docs for why the two models
//! are not unified in this slice. `/v1/watches` covers
//! create/list/get/update/pause/resume/delete.
//! `POST /v1/watches/{id}/exec` remains a tracked follow-up.

use axon_api::source::{WatchId, WatchListRequest, WatchRequest, WatchUpdateRequest};
use axon_core::config::Config;
use axon_services::watch as watch_svc;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
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

#[utoipa::path(
    post,
    path = "/v1/watches",
    request_body = WatchRequest,
    responses(
        (status = 200, description = "Created (or dual-write-ensured) watch detail", body = serde_json::Value),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn create_watch(
    State((state, cfg)): State<WebState>,
    Json(request): Json<WatchRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let pool = state.service_context.jobs.sqlite_pool();
    let created = watch_svc::create_source_watch(&cfg, pool.as_deref(), request)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!(created)))
}

#[utoipa::path(
    get,
    path = "/v1/watches",
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
    patch,
    path = "/v1/watches/{watch_id}",
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
