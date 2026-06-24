//! Family 4: admin / destructive routes — migrate, dedupe, watch CRUD.
//!
//! Migrate and dedupe carry the `unconditional` flavor of the scope guard:
//! they require a valid token + axon:write scope **even** in `LoopbackDev`
//! mode (see `auth::ScopeGuard::admin_write` and the matching invariant in
//! `src/web/actions.rs::authorize_action`). Watch routes use the standard
//! read/write guards.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use axon_services::system as system_svc;
use axon_services::watch::WatchDefCreateRequest;
use axon_services::{migrate as migrate_svc, watch as watch_svc};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
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
    if let Err(e) = axon_core::config::validate_collection_name(&req.from) {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", format!("from: {e}"));
    }
    if let Err(e) = axon_core::config::validate_collection_name(&req.to) {
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
    headers: HeaderMap,
    body: String,
) -> Response {
    let mut req_cfg = (*state.cfg).clone();
    let body = match parse_optional_json_body::<DedupeBody>(&headers, &body) {
        Ok(body) => body,
        Err((status, kind, message)) => return rest_error(status, kind, message),
    };
    if let Some(DedupeBody {
        collection: Some(col),
    }) = body
    {
        if let Err(e) = axon_core::config::validate_collection_name(&col) {
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

fn parse_optional_json_body<T>(
    headers: &HeaderMap,
    body: &str,
) -> Result<Option<T>, (StatusCode, &'static str, String)>
where
    T: serde::de::DeserializeOwned,
{
    if body.is_empty() {
        return Ok(None);
    }
    if !has_json_content_type(headers) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "non-empty request body must use application/json".to_string(),
        ));
    }
    serde_json::from_str(body).map(Some).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid JSON request body: {e}"),
        )
    })
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
    let pool = match state.watch_pool().await {
        Ok(pool) => pool,
        Err(err) => return map_service_error(err.as_ref() as &(dyn std::error::Error + 'static)),
    };
    match watch_svc::list_watch_defs_with_pool(pool.as_ref(), limit).await {
        Ok(defs) => Json(serde_json::json!({ "watches": defs })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

/// Maximum serialized size for task_payload — guards against storage abuse.
const MAX_TASK_PAYLOAD_BYTES: usize = 64 * 1024;

pub(crate) async fn v1_watch_create(
    State(state): State<RestState>,
    Json(input): Json<WatchDefCreateRequest>,
) -> Response {
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
    let input = match input.into_create() {
        Ok(input) => input,
        Err(msg) => return rest_error(StatusCode::BAD_REQUEST, "bad_request", msg),
    };
    let pool = match state.watch_pool().await {
        Ok(pool) => pool,
        Err(err) => return map_service_error(err.as_ref() as &(dyn std::error::Error + 'static)),
    };
    match watch_svc::create_watch_def_with_pool(pool.as_ref(), &input).await {
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
    let pool = match state.watch_pool().await {
        Ok(pool) => pool,
        Err(err) => return map_service_error(err.as_ref() as &(dyn std::error::Error + 'static)),
    };
    match watch_svc::get_watch_def_with_pool(pool.as_ref(), watch_id).await {
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
    let pool = match state.watch_pool().await {
        Ok(pool) => pool,
        Err(err) => return map_service_error(err.as_ref() as &(dyn std::error::Error + 'static)),
    };
    let def = match watch_svc::get_watch_def_with_pool(pool.as_ref(), watch_id).await {
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
    match watch_svc::run_watch_now_with_pool(state.cfg.as_ref(), pool.as_ref(), &def).await {
        Ok(run) => (StatusCode::ACCEPTED, Json(run)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}
