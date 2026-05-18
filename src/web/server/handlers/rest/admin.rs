//! Family 4: admin / destructive routes — migrate, dedupe, watch CRUD.
//!
//! Migrate and dedupe carry the `unconditional` flavor of the scope guard:
//! they require a valid token + axon:write scope **even** in `LoopbackDev`
//! mode (see `auth::ScopeGuard::admin_write` and the matching invariant in
//! `src/web/actions.rs::authorize_action`). Watch routes use the standard
//! read/write guards.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use crate::jobs::watch_lite::WatchDefCreate;
use crate::services::system as system_svc;
use crate::services::{migrate as migrate_svc, watch as watch_svc};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

#[allow(clippy::result_large_err)]
fn parse_uuid(id: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(id).map_err(|_| {
        rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid watch id: {id}"),
        )
    })
}

// ── migrate ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrateBody {
    pub from: String,
    pub to: String,
}

pub(crate) async fn v1_migrate(
    State(state): State<RestState>,
    Json(req): Json<MigrateBody>,
) -> Response {
    if req.from.trim().is_empty() {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "from is required".into(),
        );
    }
    if req.to.trim().is_empty() {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "to is required".into(),
        );
    }
    let mut req_cfg = (*state.cfg).clone();
    // services::migrate::migrate() reads from cfg.positional[0] / [1]
    req_cfg.positional = vec![req.from.clone(), req.to.clone()];
    match migrate_svc::migrate(&req_cfg).await {
        Ok(result) => Json(serde_json::json!({
            "from": result.from,
            "to": result.to,
            "points_migrated": result.points_migrated,
            "pages_processed": result.pages_processed,
        }))
        .into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

// ── dedupe ───────────────────────────────────────────────────────────────

pub(crate) async fn v1_dedupe(State(state): State<RestState>) -> Response {
    match system_svc::dedupe(state.cfg.as_ref(), None).await {
        Ok(result) => Json(serde_json::json!({
            "completed": result.completed,
            "duplicate_groups": result.duplicate_groups,
            "deleted": result.deleted,
        }))
        .into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

// ── watch ────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub(crate) struct WatchListParams {
    #[serde(default)]
    pub limit: Option<i64>,
}

const DEFAULT_WATCH_LIMIT: i64 = 50;
const MAX_WATCH_LIMIT: i64 = 500;

pub(crate) async fn v1_watch_list(
    State(state): State<RestState>,
    Query(params): Query<WatchListParams>,
) -> Response {
    let limit = params
        .limit
        .unwrap_or(DEFAULT_WATCH_LIMIT)
        .clamp(1, MAX_WATCH_LIMIT);
    match watch_svc::list_watch_defs(state.cfg.as_ref(), limit).await {
        Ok(defs) => Json(serde_json::json!({ "watches": defs })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_watch_create(
    State(state): State<RestState>,
    Json(input): Json<WatchDefCreate>,
) -> Response {
    match watch_svc::create_watch_def(state.cfg.as_ref(), &input).await {
        Ok(def) => (StatusCode::CREATED, Json(def)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_watch_get(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let watch_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match watch_svc::get_watch_def(state.cfg.as_ref(), watch_id).await {
        Ok(Some(def)) => Json(def).into_response(),
        Ok(None) => rest_error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("watch {watch_id} not found"),
        ),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_watch_run_now(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let watch_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let def = match watch_svc::get_watch_def(state.cfg.as_ref(), watch_id).await {
        Ok(Some(def)) => def,
        Ok(None) => {
            return rest_error(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("watch {watch_id} not found"),
            );
        }
        Err(err) => return map_service_error(err.as_ref()),
    };
    match watch_svc::run_watch_now(state.cfg.as_ref(), &def).await {
        Ok(run) => (StatusCode::ACCEPTED, Json(run)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}
