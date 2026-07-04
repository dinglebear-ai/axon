//! Family 2: synchronous POST routes with typed bodies.
//!
//! One handler per resource:
//!   - POST /v1/query, /v1/retrieve, /v1/map    — read scope
//!   - POST /v1/suggest, /v1/search,
//!     /v1/research, /v1/sources                — write scope
//!
//! NOTE: `/v1/evaluate` is intentionally absent — `services::query::evaluate`
//! holds non-`Send` errors across `.await` points; see the comment in rest.rs.
//!
//! Each handler validates input, calls the matching `services::*` function,
//! and serializes the typed result as JSON. Errors flow through
//! `error::map_service_error`.

use super::super::super::json::Json;
use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::{MapBody, QueryBody, RetrieveBody, SearchBody, SuggestBody, UrlsBody};
use axon_services::query as query_svc;
use axon_services::search as search_svc;
use axon_services::transport;
use axon_services::{map as map_svc, summarize as summarize_svc};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
fn parse_time_range(value: &str) -> Result<axon_services::types::ServiceTimeRange, Response> {
    transport::parse_service_time_range(value)
        .map_err(|message| rest_error(StatusCode::BAD_REQUEST, "bad_request", message))
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
fn require_field(value: &str, field: &'static str) -> Result<(), Response> {
    if value.trim().is_empty() {
        return Err(rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("{field} is required"),
        ));
    }
    Ok(())
}

pub(crate) async fn v1_query(
    State(state): State<RestState>,
    Json(req): Json<QueryBody>,
) -> Response {
    if let Err(r) = require_field(&req.query, "query") {
        return r;
    }
    let cfg = match super::super::rag::with_query_overrides(
        state.cfg.as_ref(),
        req.collection,
        req.since,
        req.before,
        req.hybrid_search,
    ) {
        Ok(cfg) => cfg,
        Err(err) => return err.into_response(),
    };
    let opts = transport::pagination(req.limit, req.offset, cfg.search_limit);
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => {
            return rest_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                format!("service context: {err}"),
            );
        }
    };
    match query_svc::query(&ctx, &cfg, &req.query, opts).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_retrieve(
    State(state): State<RestState>,
    Json(req): Json<RetrieveBody>,
) -> Response {
    if let Err(r) = require_field(&req.url, "url") {
        return r;
    }
    let cfg = match super::super::rag::with_query_overrides(
        state.cfg.as_ref(),
        req.collection,
        req.since,
        req.before,
        None,
    ) {
        Ok(cfg) => cfg,
        Err(err) => return err.into_response(),
    };
    let opts = transport::retrieve_options(req.max_points, req.cursor, req.token_budget);
    match query_svc::retrieve(&cfg, &req.url, opts).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(&*err),
    }
}

pub(crate) async fn v1_suggest(
    State(state): State<RestState>,
    Json(req): Json<SuggestBody>,
) -> Response {
    let cfg = match super::super::rag::with_collection_override(state.cfg.as_ref(), req.collection)
    {
        Ok(cfg) => cfg,
        Err(err) => return err.into_response(),
    };
    match query_svc::suggest(&cfg, req.focus.as_deref()).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_map(State(state): State<RestState>, Json(req): Json<MapBody>) -> Response {
    if let Err(r) = require_field(&req.url, "url") {
        return r;
    }
    let opts = transport::map_options(req.limit, req.offset);
    match map_svc::discover(state.cfg.as_ref(), &req.url, opts, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

/// Wire shape: `{ "results": [...] }`. `SearchResult.results` is a flat
/// `Vec<serde_json::Value>` and would serialize as a bare JSON array; the
/// wrapper object keeps the response future-extensible with metadata fields.
/// `/v1/research` differs intentionally — it returns the synthesized payload
/// (an object with citations/summary) directly.
pub(crate) async fn v1_search(
    State(state): State<RestState>,
    Json(req): Json<SearchBody>,
) -> Response {
    if let Err(r) = require_field(&req.query, "query") {
        return r;
    }
    let time_range = match req.time_range.as_deref() {
        Some(v) => match parse_time_range(v) {
            Ok(tr) => Some(tr),
            Err(r) => return r,
        },
        None => None,
    };
    let opts = transport::search_options(req.limit, req.offset, time_range, state.cfg.search_limit);
    match search_svc::search(state.cfg.as_ref(), &req.query, opts, None).await {
        Ok(result) => Json(serde_json::json!({ "results": result.results })).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_research(
    State(state): State<RestState>,
    Json(req): Json<SearchBody>,
) -> Response {
    if let Err(r) = require_field(&req.query, "query") {
        return r;
    }
    let time_range = match req.time_range.as_deref() {
        Some(v) => match parse_time_range(v) {
            Ok(tr) => Some(tr),
            Err(r) => return r,
        },
        None => None,
    };
    let opts = transport::search_options(req.limit, req.offset, time_range, state.cfg.search_limit);
    let service_context = match state.service_context().await {
        Ok(context) => context,
        Err(err) => return map_service_error(err.as_ref()),
    };
    match search_svc::research_with_context(
        state.cfg.as_ref(),
        &service_context,
        &req.query,
        opts,
        None,
    )
    .await
    {
        Ok(result) => Json(result.payload).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_sources(
    State(state): State<RestState>,
    Json(req): Json<axon_api::source::SourceRequest>,
) -> Response {
    if let Err(r) = require_field(&req.source, "source") {
        return r;
    }
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(err.as_ref()),
    };
    // `index_source` is not `Send` (the web-source bridge holds a
    // `Box<dyn Error>` across `.await`); run it on a blocking thread whose
    // `JoinHandle` is `Send`, mirroring `handlers::sources::index_source`.
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        handle.block_on(async move {
            axon_services::index_source(req, ctx.as_ref())
                .await
                .map_err(|err| err.to_string())
        })
    })
    .await;
    match result {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(message)) => rest_error(StatusCode::BAD_GATEWAY, "upstream", message),
        Err(err) => rest_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("source indexing task failed: {err}"),
        ),
    }
}

pub(crate) async fn v1_summarize(
    State(state): State<RestState>,
    Json(req): Json<UrlsBody>,
) -> Response {
    let urls = urls_body(req);
    if urls.is_empty() {
        return rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "url or urls is required".to_string(),
        );
    }
    match summarize_svc::summarize(state.cfg.as_ref(), &urls, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

fn urls_body(req: UrlsBody) -> Vec<String> {
    req.urls
        .unwrap_or_default()
        .into_iter()
        .chain(req.url)
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .collect()
}
