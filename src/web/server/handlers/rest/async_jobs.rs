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
//! lazy `ServiceContext` (with workers) used by `/v1/actions`.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::{CrawlSubmitBody, EmbedSubmitBody, ExtractSubmitBody};
use crate::services::context::ServiceContext;
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
use std::sync::Arc;
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────

fn missing_field(field: &'static str) -> Response {
    rest_error(
        StatusCode::BAD_REQUEST,
        "bad_request",
        format!("{field} is required"),
    )
}

fn not_found(kind: &'static str, id: Uuid) -> Response {
    rest_error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{kind} job {id} not found"),
    )
}

/// Lazily fetch the shared [`ServiceContext`], mapping init errors to a
/// REST response so callers can `?` out.
#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
async fn ctx_only(state: &RestState) -> Result<Arc<ServiceContext>, Response> {
    state
        .service_context()
        .await
        .map_err(|err| map_service_error(&*err))
}

/// Combined extractor for the status/cancel handler shape: parse the path
/// `{id}` as a UUID and fetch the [`ServiceContext`] in one go.
#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
async fn ctx_and_job_id(
    state: &RestState,
    id: &str,
) -> Result<(Arc<ServiceContext>, Uuid), Response> {
    let job_id = Uuid::parse_str(id).map_err(|_| {
        rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid job id: {id}"),
        )
    })?;
    let ctx = ctx_only(state).await?;
    Ok((ctx, job_id))
}

fn cancel_response(canceled: bool) -> Response {
    Json(serde_json::json!({ "canceled": canceled })).into_response()
}

// ── crawl ────────────────────────────────────────────────────────────────

/// Validate a slice of submitted URLs before enqueue. MCP applies this at
/// submit time; the REST surface must match so private-IP URLs are rejected
/// with a 400 rather than accepted (202) then silently failing in the worker.
fn validate_urls(urls: &[String]) -> Result<(), String> {
    for url in urls {
        crate::core::http::validate_url(url).map_err(|e| format!("{url}: {e}"))?;
    }
    Ok(())
}

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
    match crawl_svc::crawl_start_with_context(state.cfg.as_ref(), &req.urls, &ctx, None).await {
        Ok(outcome) => (StatusCode::ACCEPTED, Json(outcome)).into_response(),
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

/// Validate an embed `input` string with full parity to the MCP handler:
/// - http(s) URL → SSRF check via `validate_url`
/// - Non-existent path → pass through (service validates further)
/// - Existing local path → must be under AXON_MCP_EMBED_ALLOWED_ROOTS,
///   must not be a symlink, must not contain dotfile components
///
/// This mirrors `validate_mcp_embed_input_with_roots` in
/// `src/mcp/server/common.rs` so both surfaces apply identical guards.
fn validate_embed_input(input: &str) -> Result<(), String> {
    let input = input.trim();
    if input.starts_with("http://") || input.starts_with("https://") {
        return crate::core::http::validate_url(input).map_err(|e| e.to_string());
    }
    let path = std::path::Path::new(input);
    if !path.exists() {
        return Ok(());
    }
    // Reject symlinks before canonicalization (canonicalize follows them).
    if std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("local embed path must not be a symlink".into());
    }
    let allowed_roots: Vec<std::path::PathBuf> = std::env::var("AXON_MCP_EMBED_ALLOWED_ROOTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|p| {
                    let t = p.trim();
                    (!t.is_empty()).then(|| std::path::PathBuf::from(t))
                })
                .collect()
        })
        .unwrap_or_default();
    if allowed_roots.is_empty() {
        return Err(
            "local file embedding is disabled; set AXON_MCP_EMBED_ALLOWED_ROOTS to allow specific roots".into()
        );
    }
    let canonical = std::fs::canonicalize(path).map_err(|e| format!("invalid embed path: {e}"))?;
    let root = allowed_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| canonical.starts_with(root))
        .ok_or_else(|| {
            format!(
                "local embed path must be under one of AXON_MCP_EMBED_ALLOWED_ROOTS; got: {input}"
            )
        })?;
    // Reject dotfile components (e.g. ~/.ssh, .env) — same check as MCP.
    let relative = canonical
        .strip_prefix(&root)
        .map_err(|_| "local embed path is outside the allowed root".to_string())?;
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.') {
            return Err("local embed path must not include dotfiles".into());
        }
    }
    Ok(())
}

pub(crate) async fn v1_embed_submit(
    State(state): State<RestState>,
    Json(req): Json<EmbedSubmitBody>,
) -> Response {
    if req.input.trim().is_empty() {
        return missing_field("input");
    }
    if let Err(reason) = validate_embed_input(&req.input) {
        return rest_error(StatusCode::BAD_REQUEST, "bad_request", reason);
    }
    let ctx = match ctx_only(&state).await {
        Ok(ctx) => ctx,
        Err(r) => return r,
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
