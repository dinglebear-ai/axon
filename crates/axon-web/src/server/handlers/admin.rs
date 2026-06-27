use axon_api::mcp_schema::PurgeRequest;
use axon_core::config::Config;
use axon_services as services;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
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

pub(crate) type WatchCreateRequest = services::watch::WatchDefCreateRequest;

const MAX_TASK_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct MigrateRequest {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DedupeRequest {
    collection: Option<String>,
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
    axon_core::config::validate_collection_name(&req.from)
        .map_err(|e| HttpError::bad_request(format!("from: {e}").as_str()))?;
    axon_core::config::validate_collection_name(&req.to)
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
    request_body(content = Option<DedupeRequest>, content_type = "application/json"),
    responses(
        (status = 200, description = "Dedupe result", body = serde_json::Value),
        (status = 400, description = "Invalid dedupe request", body = crate::server::error::ErrorBody),
        (status = 415, description = "Unsupported request body content type", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn dedupe(
    State((_state, cfg)): State<WebState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<services::types::DedupeResult>, HttpError> {
    let mut req_cfg = (*cfg).clone();
    if let Some(req) = parse_optional_json_body::<DedupeRequest>(&headers, &body)?
        && let Some(collection) = req.collection
    {
        axon_core::config::validate_collection_name(&collection)
            .map_err(|e| HttpError::bad_request(format!("collection: {e}").as_str()))?;
        req_cfg.collection = collection;
    }
    services::system::dedupe(&req_cfg, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/purge",
    request_body = PurgeRequest,
    responses(
        (status = 200, description = "Purge result (counts of points/URLs matched or deleted)", body = axon_api::PurgeResult),
        (status = 400, description = "Invalid purge request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn purge(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<PurgeRequest>,
) -> Result<Json<services::types::PurgeResult>, HttpError> {
    let target = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| HttpError::bad_request("target is required"))?;
    let mut req_cfg = (*cfg).clone();
    if let Some(collection) = req.collection {
        axon_core::config::validate_collection_name(&collection)
            .map_err(|e| HttpError::bad_request(format!("collection: {e}").as_str()))?;
        req_cfg.collection = collection;
    }
    services::system::purge(&req_cfg, &target, req.prefix, req.dry_run.unwrap_or(true))
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

fn parse_optional_json_body<T>(headers: &HeaderMap, body: &str) -> Result<Option<T>, HttpError>
where
    T: serde::de::DeserializeOwned,
{
    if body.is_empty() {
        return Ok(None);
    }
    if !has_json_content_type(headers) {
        return Err(HttpError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "non-empty request body must use application/json",
        ));
    }
    serde_json::from_str(body)
        .map(Some)
        .map_err(|e| HttpError::bad_request(format!("invalid JSON request body: {e}")))
}

fn has_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let media_type = value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            media_type == "application/json" || media_type.ends_with("+json")
        })
        .unwrap_or(false)
}

#[utoipa::path(
    get,
    path = "/v1/watch",
    params(WatchListQuery),
    responses(
        (status = 200, description = "Watch definitions", body = serde_json::Value),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
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
        (status = 400, description = "Invalid watch request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch storage unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "watch"
)]
pub(crate) async fn create_watch(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<WatchCreateRequest>,
) -> Result<Json<services::watch::WatchDef>, HttpError> {
    if req
        .task_payload
        .as_str()
        .map_or_else(|| req.task_payload.to_string(), |s| s.to_string())
        .len()
        > MAX_TASK_PAYLOAD_BYTES
    {
        return Err(HttpError::bad_request("task_payload exceeds 64 KiB limit"));
    }
    let input = req
        .into_create()
        .map_err(|msg| HttpError::bad_request(&msg))?;
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
        (status = 404, description = "Watch not found", body = crate::server::error::ErrorBody),
        (status = 502, description = "Watch execution failed", body = crate::server::error::ErrorBody)
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
