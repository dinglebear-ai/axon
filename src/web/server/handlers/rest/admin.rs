//! Family 4: admin / destructive routes — migrate, dedupe, watch CRUD.
//!
//! Migrate and dedupe carry the `unconditional` flavor of the scope guard:
//! they require a valid token + axon:write scope **even** in `LoopbackDev`
//! mode (see `auth::ScopeGuard::admin_write` and the matching invariant in
//! `src/web/actions.rs::authorize_action`). Watch routes use the standard
//! read/write guards.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use crate::services::system as system_svc;
use crate::services::watch::WatchDefCreate;
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
    if req.from == req.to {
        // Guard against a self-migration triggering a destructive long-running
        // operation on the live collection.
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "from and to must be different collections".into(),
        );
    }
    // Validate collection names before they reach Qdrant URL interpolation —
    // migrate.rs builds URLs like `{qdrant_url}/collections/{from}/points/scroll`
    // without percent-encoding. The same guard the MCP path applies.
    if let Err(e) = crate::core::config::validate_collection_name(&req.from) {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", format!("from: {e}"));
    }
    if let Err(e) = crate::core::config::validate_collection_name(&req.to) {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", format!("to: {e}"));
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

/// Optional body for dedupe — allows overriding the target collection,
/// matching the MCP action's `DedupeRequest.collection` field.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct DedupeBody {
    #[serde(default)]
    pub collection: Option<String>,
}

pub(crate) async fn v1_dedupe(
    State(state): State<RestState>,
    body: Option<Json<DedupeBody>>,
) -> Response {
    let mut req_cfg = (*state.cfg).clone();
    if let Some(Json(DedupeBody {
        collection: Some(col),
    })) = body
    {
        if let Err(e) = crate::core::config::validate_collection_name(&col) {
            return rest_error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                format!("collection: {e}"),
            );
        }
        req_cfg.collection = col;
    }
    match system_svc::dedupe(&req_cfg, None).await {
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

const MIN_WATCH_INTERVAL_SECS: i64 = 30;
const MAX_WATCH_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60; // 7 days
/// Maximum serialized size for task_payload — guards against storage abuse.
const MAX_TASK_PAYLOAD_BYTES: usize = 64 * 1024;
/// Task types supported by the watch scheduler. Any value not in this list
/// will fail at execution time; reject early with 400 so callers learn
/// immediately rather than on the first run attempt.
const SUPPORTED_TASK_TYPES: &[&str] = &["refresh"];

pub(crate) async fn v1_watch_create(
    State(state): State<RestState>,
    Json(input): Json<WatchDefCreate>,
) -> Response {
    if input.name.trim().is_empty() {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "name is required".into(),
        );
    }
    // Reject leading/trailing whitespace — the stored task_type would fail
    // to match SUPPORTED_TASK_TYPES at execution time since run_watch_now
    // compares verbatim. Reject here so callers get a clear error immediately.
    if input.task_type != input.task_type.trim() {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "task_type must not have leading or trailing whitespace".into(),
        );
    }
    if !SUPPORTED_TASK_TYPES.contains(&input.task_type.as_str()) {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!(
                "unsupported task_type: '{}'; supported: {}",
                input.task_type,
                SUPPORTED_TASK_TYPES.join(", ")
            ),
        );
    }
    if input.every_seconds < MIN_WATCH_INTERVAL_SECS
        || input.every_seconds > MAX_WATCH_INTERVAL_SECS
    {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!(
                "every_seconds must be between {MIN_WATCH_INTERVAL_SECS} and {MAX_WATCH_INTERVAL_SECS}"
            ),
        );
    }
    // Guard against storage abuse via oversized payloads.
    if serde_json::to_string(&input.task_payload)
        .map(|s| s.len())
        .unwrap_or(usize::MAX)
        > MAX_TASK_PAYLOAD_BYTES
    {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("task_payload exceeds {MAX_TASK_PAYLOAD_BYTES} byte limit"),
        );
    }
    // For "refresh" tasks, task_payload.urls must be a non-empty string array
    // and all URLs must pass SSRF validation. Validate at create time so the
    // watch definition is rejected immediately rather than failing silently
    // on every scheduled run.
    if input.task_type.as_str() == "refresh" {
        let urls = input
            .task_payload
            .get("urls")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                rest_error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "task_payload.urls is required for refresh tasks and must be an array".into(),
                )
            });
        let urls = match urls {
            Ok(u) => u,
            Err(r) => return r,
        };
        if urls.is_empty() {
            return rest_error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "task_payload.urls must not be empty for refresh tasks".into(),
            );
        }
        for url_val in urls {
            let url = url_val.as_str().ok_or_else(|| {
                rest_error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "task_payload.urls entries must be strings".into(),
                )
            });
            let url = match url {
                Ok(u) => u,
                Err(r) => return r,
            };
            if let Err(e) = crate::core::http::validate_url(url) {
                return rest_error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    format!("invalid url in task_payload.urls: {e}"),
                );
            }
        }
    }
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
