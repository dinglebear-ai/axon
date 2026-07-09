//! Family 3: async extract job routes — POST submit + GET status + POST .../cancel.
//!
//! For extract:
//!   - POST /v1/extract             — submit, returns 202 + JobStartOutcome
//!   - GET  /v1/extract/{id}        — status, 200 + result JSON (404 if unknown)
//!   - POST /v1/extract/{id}/cancel — cancel, 200 + { canceled: bool }
//!
//! Submit and cancel are `axon:write` scope-gated; GET status uses the
//! `axon:read` guard shared in `rest.rs`. Cancel is `POST .../cancel`
//! rather than `DELETE /{id}` so the GET (read) and cancel (write) routes
//! can carry distinct scope-guard layers — axum 0.8 `MethodRouter` layers
//! apply across all methods on a single path.
//!
//! The handlers go through `RestState::service_context()` to share the same
//! lazy `ServiceContext` (with workers) used by the unified server runtime.
//!
//! The legacy crawl / embed / ingest job families were removed in favor of the
//! unified `POST /v1/sources` entrypoint (see `rest/sync_post.rs::v1_sources`).

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::ExtractSubmitBody;
use axon_services::extract as extract_svc;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[path = "async_jobs/helpers.rs"]
mod helpers;
use helpers::{
    cancel_response, count_response, ctx_and_job_id, ctx_only, missing_field, not_found,
    validate_urls,
};

// ── extract ──────────────────────────────────────────────────────────────

pub(crate) async fn v1_extract_submit(
    State(state): State<RestState>,
    Json(req): Json<ExtractSubmitBody>,
) -> Response {
    if req.urls.is_empty() {
        return missing_field("urls");
    }
    if let Err(reason) = validate_urls(&req.urls) {
        return rest_error(StatusCode::BAD_REQUEST, "invalid_url", reason);
    }
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    let mut cfg = state.cfg.as_ref().clone();
    if let Some(max_pages) = req.max_pages {
        cfg.max_pages = max_pages;
    }
    if let Some(render_mode) = req.render_mode {
        cfg.render_mode = render_mode;
    }
    if let Some(embed) = req.embed {
        cfg.embed = embed;
    }
    cfg.custom_headers.extend(
        req.headers
            .into_iter()
            .map(|(key, value)| format!("{key}: {value}")),
    );
    // Test-only scaffolding router (see module doc in rest.rs) — not mounted
    // in production, so there is no real per-request auth context to pass.
    match extract_svc::extract_start_with_context(&cfg, &req.urls, req.prompt, &ctx, None, None)
        .await
    {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_list(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match extract_svc::extract_list(&ctx, 100, 0).await {
        Ok(jobs) => Json(jobs).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_cleanup(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match extract_svc::extract_cleanup(&ctx).await {
        Ok(count) => count_response("cleaned", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_clear(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match extract_svc::extract_clear(&ctx).await {
        Ok(count) => count_response("cleared", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_recover(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match extract_svc::extract_recover(&ctx).await {
        Ok(count) => count_response("recovered", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match extract_svc::extract_status(&ctx, job_id).await {
        Ok(Some(result)) => Json(result.payload).into_response(),
        Ok(None) => not_found("extract", job_id),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_cancel(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match extract_svc::extract_cancel(&ctx, job_id).await {
        Ok(canceled) => cancel_response(canceled),
        Err(err) => map_service_error(err.as_ref()),
    }
}
