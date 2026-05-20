//! Family 3: async job routes — POST submit + GET status + POST .../cancel per kind.
//!
//! For each of crawl / embed / extract / ingest:
//!   - POST /v1/{kind}             — submit, returns 202 + JobStartOutcome
//!   - GET  /v1/{kind}/{id}        — status, 200 + result JSON (404 if unknown)
//!   - POST /v1/{kind}/{id}/cancel — cancel, 200 + { canceled: bool }
//!
//! Submit and cancel are `axon:write` scope-gated; GET status uses the
//! `axon:read` guard shared in `rest.rs`. Cancel is `POST .../cancel`
//! rather than `DELETE /{id}` so the GET (read) and cancel (write) routes
//! can carry distinct scope-guard layers — axum 0.8 `MethodRouter` layers
//! apply across all methods on a single path.
//!
//! The handlers go through `RestState::service_context()` to share the same
//! lazy `ServiceContext` (with workers) used by the unified server runtime.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::{CrawlSubmitBody, EmbedSubmitBody, ExtractSubmitBody};
use crate::services::ingest::IngestSource;
use crate::services::{
    crawl as crawl_svc, embed as embed_svc, extract as extract_svc, ingest as ingest_svc,
};
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
    validate_embed_input, validate_urls,
};

// ── crawl ────────────────────────────────────────────────────────────────

pub(crate) async fn v1_crawl_submit(
    State(state): State<RestState>,
    Json(req): Json<CrawlSubmitBody>,
) -> Response {
    if req.urls.is_empty() {
        return missing_field("urls");
    }
    if let Err(reason) = validate_urls(&req.urls) {
        // 400 BAD_REQUEST — invalid client input (SSRF-blocked URL).
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
    if let Some(max_depth) = req.max_depth {
        cfg.max_depth = max_depth;
    }
    if let Some(render_mode) = req.render_mode {
        cfg.render_mode = render_mode;
    }
    if let Some(include_subdomains) = req.include_subdomains {
        cfg.include_subdomains = include_subdomains;
    }
    if let Some(respect_robots) = req.respect_robots {
        cfg.respect_robots = respect_robots;
    }
    if let Some(discover_sitemaps) = req.discover_sitemaps {
        cfg.discover_sitemaps = discover_sitemaps;
    }
    if let Some(max_sitemaps) = req.max_sitemaps {
        cfg.max_sitemaps = max_sitemaps;
    }
    if let Some(sitemap_since_days) = req.sitemap_since_days {
        cfg.sitemap_since_days = sitemap_since_days;
    }
    if let Some(delay_ms) = req.delay_ms {
        cfg.delay_ms = delay_ms;
    }
    cfg.custom_headers.extend(
        req.headers
            .into_iter()
            .map(|(key, value)| format!("{key}: {value}")),
    );
    match crawl_svc::crawl_start_with_context(&cfg, &req.urls, &ctx, None).await {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_list(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match crawl_svc::crawl_list(&ctx, 100, 0).await {
        Ok(jobs) => Json(jobs).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_cleanup(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match crawl_svc::crawl_cleanup(&ctx).await {
        Ok(count) => count_response("cleaned", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_clear(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match crawl_svc::crawl_clear(&ctx).await {
        Ok(count) => count_response("cleared", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_recover(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match crawl_svc::crawl_recover(&ctx).await {
        Ok(count) => count_response("recovered", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match crawl_svc::crawl_status(&ctx, job_id).await {
        Ok(Some(result)) => Json(result.payload).into_response(),
        Ok(None) => not_found("crawl", job_id),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_crawl_cancel(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match crawl_svc::crawl_cancel(&ctx, job_id).await {
        Ok(canceled) => cancel_response(canceled),
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
    let input_for_validation = req.input.clone();
    let validation =
        tokio::task::spawn_blocking(move || validate_embed_input(&input_for_validation))
            .await
            .map_err(|err| format!("embed input validation task failed: {err}"))
            .and_then(|result| result);
    if let Err(reason) = validation {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", reason);
    }
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    let mut cfg = state.cfg.as_ref().clone();
    if let Some(collection) = req.collection {
        cfg.collection = collection;
    }
    match embed_svc::embed_start_with_context(
        &cfg,
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

pub(crate) async fn v1_embed_list(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match embed_svc::embed_list(&ctx, 100, 0).await {
        Ok(jobs) => Json(jobs).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_cleanup(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match embed_svc::embed_cleanup(&ctx).await {
        Ok(count) => count_response("cleaned", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_clear(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match embed_svc::embed_clear(&ctx).await {
        Ok(count) => count_response("cleared", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_recover(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match embed_svc::embed_recover(&ctx).await {
        Ok(count) => count_response("recovered", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_embed_status(
    State(state): State<RestState>,
    Path(id): Path<String>,
) -> Response {
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
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
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match embed_svc::embed_cancel(&ctx, job_id).await {
        Ok(canceled) => cancel_response(canceled),
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
    match extract_svc::extract_start_with_context(&cfg, &req.urls, req.prompt, &ctx, None).await {
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

pub(crate) async fn v1_ingest_list(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match ingest_svc::ingest_list(&ctx, 100, 0).await {
        Ok(jobs) => Json(jobs).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_ingest_cleanup(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match ingest_svc::ingest_cleanup(&ctx).await {
        Ok(count) => count_response("cleaned", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_ingest_clear(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match ingest_svc::ingest_clear(&ctx).await {
        Ok(count) => count_response("cleared", count),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_ingest_recover(State(state): State<RestState>) -> Response {
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
    };
    match ingest_svc::ingest_recover(&ctx).await {
        Ok(count) => count_response("recovered", count),
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

// ── ingest ───────────────────────────────────────────────────────────────

pub(crate) async fn v1_ingest_submit(
    State(state): State<RestState>,
    Json(source): Json<IngestSource>,
) -> Response {
    if let Err(reason) = ingest_svc::validate_ingest_source(&source) {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", reason);
    }
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
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
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
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
    let (ctx, job_id) = match ctx_and_job_id(&state, &id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match ingest_svc::ingest_cancel(&ctx, job_id).await {
        Ok(canceled) => cancel_response(canceled),
        Err(err) => map_service_error(err.as_ref()),
    }
}
