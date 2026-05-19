use crate::core::config::Config;
use crate::services;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::post,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(crate) struct WatchListQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct WatchCreateRequest {
    name: String,
    task_type: String,
    task_payload: serde_json::Value,
    every_seconds: i64,
    enabled: Option<bool>,
    next_run_at: Option<DateTime<Utc>>,
}

/// Task types supported by the watch scheduler. Any value not in this list
/// fails at execution time; rejecting at create time gives callers immediate feedback.
const SUPPORTED_TASK_TYPES: &[&str] = &["refresh"];
const MAX_TASK_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct MigrateRequest {
    pub from: String,
    pub to: String,
}

// migrate_router is unused — migrate is wired directly in routing.rs
#[allow(dead_code)]
pub(crate) fn migrate_router<S: Clone + Send + Sync + 'static>() -> Router<S>
where
    (super::super::state::AppState, Arc<Config>): axum::extract::FromRef<S>,
{
    Router::new().route("/v1/migrate", post(migrate))
}

pub(crate) async fn migrate(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<MigrateRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    if req.from.trim().is_empty() {
        return Err(HttpError::bad_request("from is required"));
    }
    if req.to.trim().is_empty() {
        return Err(HttpError::bad_request("to is required"));
    }
    if req.from == req.to {
        return Err(HttpError::bad_request("from and to must be different"));
    }
    // Validate before Qdrant URL interpolation — migrate builds URLs like
    // {qdrant_url}/collections/{from}/points/scroll without percent-encoding.
    crate::core::config::validate_collection_name(&req.from)
        .map_err(|e| HttpError::bad_request(format!("from: {e}").as_str()))?;
    crate::core::config::validate_collection_name(&req.to)
        .map_err(|e| HttpError::bad_request(format!("to: {e}").as_str()))?;
    let mut req_cfg = (*cfg).clone();
    req_cfg.positional = vec![req.from.clone(), req.to.clone()];
    let result = services::migrate::migrate(&req_cfg)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(serde_json::json!({
        "from": result.from,
        "to": result.to,
        "points_migrated": result.points_migrated,
        "pages_processed": result.pages_processed,
    })))
}

#[utoipa::path(
    post,
    path = "/v1/dedupe",
    responses(
        (status = 200, description = "Dedupe result", body = serde_json::Value),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn dedupe(
    State((_state, cfg)): State<WebState>,
) -> Result<Json<services::types::DedupeResult>, HttpError> {
    services::system::dedupe(&cfg, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/watch",
    params(WatchListQuery),
    responses(
        (status = 200, description = "Watch definitions", body = serde_json::Value),
        (status = 502, description = "Watch storage unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn list_watch(
    State((_state, cfg)): State<WebState>,
    Query(query): Query<WatchListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let watches = services::watch::list_watch_defs(&cfg, limit)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "watches": watches, "limit": limit })))
}

#[utoipa::path(
    post,
    path = "/v1/watch",
    request_body = WatchCreateRequest,
    responses(
        (status = 200, description = "Created watch definition", body = serde_json::Value),
        (status = 400, description = "Invalid watch request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn create_watch(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<WatchCreateRequest>,
) -> Result<Json<services::watch::WatchDef>, HttpError> {
    if req.name.trim().is_empty() {
        return Err(HttpError::bad_request("name is required"));
    }
    let task_type = req.task_type.trim();
    if task_type.is_empty() {
        return Err(HttpError::bad_request("task_type is required"));
    }
    if !SUPPORTED_TASK_TYPES.contains(&task_type) {
        return Err(HttpError::bad_request(
            format!(
                "unsupported task_type '{task_type}'; supported: {}",
                SUPPORTED_TASK_TYPES.join(", ")
            )
            .as_str(),
        ));
    }
    if req.every_seconds < 1 {
        return Err(HttpError::bad_request("every_seconds must be >= 1"));
    }
    if req
        .task_payload
        .as_str()
        .map_or_else(|| req.task_payload.to_string(), |s| s.to_string())
        .len()
        > MAX_TASK_PAYLOAD_BYTES
    {
        return Err(HttpError::bad_request("task_payload exceeds 64 KiB limit"));
    }
    // Validate refresh task URLs at create time so the watch doesn't silently
    // fail every run.
    if task_type == "refresh" {
        let urls = req
            .task_payload
            .get("urls")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                HttpError::bad_request("task_payload.urls is required for refresh tasks")
            })?;
        if urls.is_empty() {
            return Err(HttpError::bad_request(
                "task_payload.urls must not be empty",
            ));
        }
        for url_val in urls {
            let url = url_val.as_str().ok_or_else(|| {
                HttpError::bad_request("task_payload.urls entries must be strings")
            })?;
            crate::core::http::validate_url(url)
                .map_err(|e| HttpError::bad_request(format!("invalid url: {e}").as_str()))?;
        }
    }
    let input = services::watch::WatchDefCreate {
        name: req.name.trim().to_string(),
        task_type: task_type.to_string(),
        task_payload: req.task_payload,
        every_seconds: req.every_seconds,
        enabled: req.enabled.unwrap_or(true),
        next_run_at: req.next_run_at.unwrap_or_else(Utc::now),
    };
    services::watch::create_watch_def(&cfg, &input)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/watch/{id}/run",
    params(("id" = uuid::Uuid, Path, description = "Watch definition ID")),
    responses(
        (status = 200, description = "Watch run result", body = serde_json::Value),
        (status = 404, description = "Watch not found", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Watch execution failed", body = crate::web::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn run_watch(
    State((_state, cfg)): State<WebState>,
    Path(id): Path<Uuid>,
) -> Result<Json<services::watch::WatchRun>, HttpError> {
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        handle.block_on(async move {
            let Some(watch) = services::watch::get_watch_def(&cfg, id)
                .await
                .map_err(|err| RunWatchError::Service(err.to_string()))?
            else {
                return Err(RunWatchError::NotFound(id));
            };
            services::watch::run_watch_now(&cfg, &watch)
                .await
                .map_err(|err| RunWatchError::Service(err.to_string()))
        })
    })
    .await
    .map_err(|err| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("watch task failed: {err}"),
        )
    })?;
    result.map(Json).map_err(RunWatchError::into_http_error)
}

enum RunWatchError {
    NotFound(Uuid),
    Service(String),
}

impl RunWatchError {
    fn into_http_error(self) -> HttpError {
        match self {
            Self::NotFound(id) => HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("watch not found: {id}"),
            ),
            Self::Service(message) => HttpError::from_error(&std::io::Error::other(message)),
        }
    }
}
