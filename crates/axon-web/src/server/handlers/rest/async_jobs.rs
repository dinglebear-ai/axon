//! Family 3: async extract job route — POST submit only.
//!
//! For extract:
//!   - POST /v1/extract             — submit, returns 202 + JobStartOutcome
//!
//! Lifecycle operations are intentionally not mounted under `/v1/extract/*`;
//! callers use the unified `/v1/jobs` routes for status, cancel, cleanup, and
//! recovery.
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
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[path = "async_jobs/helpers.rs"]
mod helpers;
use helpers::{ctx_only, missing_field, validate_urls};

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
