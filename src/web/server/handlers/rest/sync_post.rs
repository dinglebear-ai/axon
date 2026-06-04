//! Family 2: synchronous POST routes with typed bodies.
//!
//! One handler per resource:
//!   - POST /v1/query, /v1/retrieve, /v1/map    — read scope
//!   - POST /v1/suggest, /v1/search,
//!     /v1/research, /v1/scrape                 — write scope
//!
//! NOTE: `/v1/evaluate` is intentionally absent — `services::query::evaluate`
//! holds non-`Send` errors across `.await` points; see the comment in rest.rs.
//!
//! Each handler validates input, calls the matching `services::*` function,
//! and serializes the typed result as JSON. Errors flow through
//! `error::map_service_error`.

use super::error::{map_service_error, rest_error};
use super::state::RestState;
use super::types::{
    MapBody, QueryBody, RetrieveBody, ScrapeBody, SearchBody, SuggestBody, UrlsBody,
};
use crate::services::query as query_svc;
use crate::services::search as search_svc;
use crate::services::types::{
    MapOptions, Pagination, RetrieveOptions, SearchOptions, ServiceTimeRange,
};
use crate::services::{map as map_svc, scrape as scrape_svc, summarize as summarize_svc};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 1000;

fn embed_scrape_doc_sync(
    cfg: &crate::core::config::Config,
    doc: crate::vector::ops::PreparedDoc,
) -> Result<(), String> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        handle
            .block_on(crate::vector::ops::embed_prepared_docs(
                cfg,
                vec![doc],
                None,
            ))
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
}

fn pagination(limit: Option<usize>, offset: Option<usize>) -> Pagination {
    Pagination {
        limit: limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: offset.unwrap_or(0),
    }
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
fn parse_time_range(value: &str) -> Result<ServiceTimeRange, Response> {
    match value.to_ascii_lowercase().as_str() {
        "day" => Ok(ServiceTimeRange::Day),
        "week" => Ok(ServiceTimeRange::Week),
        "month" => Ok(ServiceTimeRange::Month),
        "year" => Ok(ServiceTimeRange::Year),
        other => Err(rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid time_range: {other}; expected day|week|month|year"),
        )),
    }
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
    let opts = pagination(req.limit, req.offset);
    match query_svc::query(state.cfg.as_ref(), &req.query, opts).await {
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
    let opts = RetrieveOptions {
        max_points: req.max_points,
        cursor: req.cursor,
        token_budget: req.token_budget,
    };
    match query_svc::retrieve(state.cfg.as_ref(), &req.url, opts).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(&*err),
    }
}

pub(crate) async fn v1_suggest(
    State(state): State<RestState>,
    Json(req): Json<SuggestBody>,
) -> Response {
    match query_svc::suggest(state.cfg.as_ref(), req.focus.as_deref()).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_map(State(state): State<RestState>, Json(req): Json<MapBody>) -> Response {
    if let Err(r) = require_field(&req.url, "url") {
        return r;
    }
    let opts = MapOptions {
        limit: req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: req.offset.unwrap_or(0),
    };
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
    let opts = SearchOptions {
        limit: req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: req.offset.unwrap_or(0),
        time_range,
    };
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
    let opts = SearchOptions {
        limit: req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: req.offset.unwrap_or(0),
        time_range,
    };
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

pub(crate) async fn v1_scrape(
    State(state): State<RestState>,
    Json(req): Json<ScrapeBody>,
) -> Response {
    if let Err(r) = require_field(&req.url, "url") {
        return r;
    }
    let mut cfg = state.cfg.as_ref().clone();
    if let Some(embed) = req.embed {
        cfg.embed = embed;
    }
    match scrape_svc::scrape(&cfg, &req.url, None).await {
        Ok(result) => {
            let doc = cfg
                .embed
                .then(|| crate::cli::commands::scrape::scrape_result_to_prepared_doc(&result));
            if cfg.embed
                && let Some(doc) = doc
                && let Err(err) = embed_scrape_doc_sync(&cfg, doc)
            {
                return rest_error(StatusCode::BAD_GATEWAY, "upstream_error", err);
            }
            Json(result).into_response()
        }
        Err(err) => map_service_error(err.as_ref()),
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
