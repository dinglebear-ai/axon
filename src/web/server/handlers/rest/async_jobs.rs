//! Family 3: async job routes — POST + GET + DELETE per job kind.
//!
//! For each of crawl / embed / extract / ingest:
//!   - POST   /v1/{kind}        — submit, returns 202 + JobStartOutcome
//!   - GET    /v1/{kind}/:id    — status, 200 + result JSON (404 if unknown)
//!   - DELETE /v1/{kind}/:id    — cancel, 200 + { canceled: bool }
//!
//! All routes are `axon:write` scope-gated except GET (read scope; uses the
//! shared `read` guard in `rest.rs`).
//!
//! The handlers go through `RestState::service_context()` to share the same
//! lazy `ServiceContext` (with workers) used by `/v1/actions`.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::{CrawlSubmitBody, EmbedSubmitBody, ExtractSubmitBody};
use crate::jobs::ingest::types::IngestSource;
use crate::services::{
    crawl as crawl_svc, embed as embed_svc, extract as extract_svc, ingest as ingest_svc,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────

fn missing_field(field: &'static str) -> Response {
    rest_error(
        StatusCode::BAD_REQUEST,
        "bad_request",
        format!("{field} is required"),
    )
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
fn parse_uuid(id: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(id).map_err(|_| {
        rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid job id: {id}"),
        )
    })
}

fn not_found(kind: &'static str, id: Uuid) -> Response {
    rest_error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{kind} job {id} not found"),
    )
}

// ── crawl ────────────────────────────────────────────────────────────────

pub(crate) async fn v1_crawl_submit(
    State(state): State<RestState>,
    Json(req): Json<CrawlSubmitBody>,
) -> Response {
    if req.urls.is_empty() {
        return missing_field("urls");
    }
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match crawl_svc::crawl_start_with_context(state.cfg.as_ref(), &req.urls, &ctx, None).await {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match crawl_svc::crawl_status(&ctx, job_id).await {
        Ok(result) => Json(result.payload).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_cancel(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match crawl_svc::crawl_cancel(&ctx, job_id).await {
        Ok(canceled) => Json(serde_json::json!({ "canceled": canceled })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

// ── embed ────────────────────────────────────────────────────────────────

pub(crate) async fn v1_embed_submit(
    State(state): State<RestState>,
    Json(req): Json<EmbedSubmitBody>,
) -> Response {
    if req.input.trim().is_empty() {
        return missing_field("input");
    }
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match embed_svc::embed_start_with_context(
        state.cfg.as_ref(),
        &req.input,
        &ctx,
        None,
        req.source_type.as_deref(),
    )
    .await
    {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match embed_svc::embed_status(&ctx, job_id).await {
        Ok(Some(result)) => Json(result.payload).into_response(),
        Ok(None) => not_found("embed", job_id),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_cancel(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match embed_svc::embed_cancel(&ctx, job_id).await {
        Ok(canceled) => Json(serde_json::json!({ "canceled": canceled })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

// ── extract ──────────────────────────────────────────────────────────────

pub(crate) async fn v1_extract_submit(
    State(state): State<RestState>,
    Json(req): Json<ExtractSubmitBody>,
) -> Response {
    if req.urls.is_empty() {
        return missing_field("urls");
    }
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match extract_svc::extract_start_with_context(
        state.cfg.as_ref(),
        &req.urls,
        req.prompt,
        &ctx,
        None,
    )
    .await
    {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_extract_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
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
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match extract_svc::extract_cancel(&ctx, job_id).await {
        Ok(canceled) => Json(serde_json::json!({ "canceled": canceled })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

// ── ingest ───────────────────────────────────────────────────────────────

pub(crate) async fn v1_ingest_submit(
    State(state): State<RestState>,
    Json(source): Json<IngestSource>,
) -> Response {
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match ingest_svc::ingest_start_with_context(state.cfg.as_ref(), source, &ctx).await {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_ingest_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match ingest_svc::ingest_status(&ctx, job_id).await {
        Ok(Some(result)) => Json(result.payload).into_response(),
        Ok(None) => not_found("ingest", job_id),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_ingest_cancel(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let job_id = match parse_uuid(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(&*err),
    };
    match ingest_svc::ingest_cancel(&ctx, job_id).await {
        Ok(canceled) => Json(serde_json::json!({ "canceled": canceled })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}
