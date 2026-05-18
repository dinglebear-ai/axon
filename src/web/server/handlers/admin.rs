use crate::core::config::Config;
use crate::services;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Deserialize)]
pub(crate) struct WatchListQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WatchCreateRequest {
    name: String,
    task_type: String,
    task_payload: serde_json::Value,
    every_seconds: i64,
    enabled: Option<bool>,
    next_run_at: Option<DateTime<Utc>>,
}

pub(crate) async fn dedupe(
    State((_state, cfg)): State<WebState>,
) -> Result<Json<services::types::DedupeResult>, HttpError> {
    services::system::dedupe(&cfg, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

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

pub(crate) async fn create_watch(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<WatchCreateRequest>,
) -> Result<Json<services::watch::WatchDef>, HttpError> {
    if req.name.trim().is_empty() {
        return Err(HttpError::bad_request("name is required"));
    }
    if req.task_type.trim().is_empty() {
        return Err(HttpError::bad_request("task_type is required"));
    }
    if req.every_seconds < 1 {
        return Err(HttpError::bad_request("every_seconds must be >= 1"));
    }
    let input = services::watch::WatchDefCreate {
        name: req.name.trim().to_string(),
        task_type: req.task_type.trim().to_string(),
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
