use crate::core::config::Config;
use crate::services;
use crate::services::types::{Pagination, RetrieveOptions};
use axum::{Json, extract::State};
use serde::Deserialize;
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct QueryRequest {
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RetrieveRequest {
    url: String,
    max_points: Option<usize>,
    cursor: Option<String>,
    token_budget: Option<usize>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct EvaluateRequest {
    question: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SuggestRequest {
    focus: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Semantic query results", body = serde_json::Value),
        (status = 400, description = "Invalid query request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn query(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<services::types::QueryResult>, HttpError> {
    let query = required_text(&req.query, "query")?;
    services::query::query(&cfg, query, pagination(req.limit, req.offset, 10, 100))
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/retrieve",
    request_body = RetrieveRequest,
    responses(
        (status = 200, description = "Stored document chunks", body = serde_json::Value),
        (status = 400, description = "Invalid retrieve request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn retrieve(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<RetrieveRequest>,
) -> Result<Json<services::types::RetrieveResult>, HttpError> {
    let url = required_text(&req.url, "url")?;
    services::query::retrieve(
        &cfg,
        url,
        RetrieveOptions {
            max_points: req.max_points,
            cursor: req.cursor,
            token_budget: req.token_budget,
        },
    )
    .await
    .map(Json)
    .map_err(HttpError::from_box_send_sync)
}

#[utoipa::path(
    post,
    path = "/v1/evaluate",
    request_body = EvaluateRequest,
    responses(
        (status = 200, description = "Evaluation result", body = serde_json::Value),
        (status = 400, description = "Invalid evaluation request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream LLM or vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn evaluate(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<services::types::EvaluateResult>, HttpError> {
    let question = required_text(&req.question, "question")?;
    services::query::evaluate(&cfg, question)
        .await
        .map(Json)
        .map_err(HttpError::from_box_send_sync)
}

#[utoipa::path(
    post,
    path = "/v1/suggest",
    request_body = SuggestRequest,
    responses(
        (status = 200, description = "Suggested URLs to crawl", body = serde_json::Value),
        (status = 502, description = "Upstream search or LLM service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn suggest(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<services::types::SuggestResult>, HttpError> {
    let focus = req
        .focus
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    services::query::suggest(&cfg, focus)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

pub(crate) fn required_text<'a>(value: &'a str, field: &'static str) -> Result<&'a str, HttpError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(HttpError::bad_request(format!("{field} is required")))
    } else {
        Ok(trimmed)
    }
}

pub(crate) fn pagination(
    limit: Option<usize>,
    offset: Option<usize>,
    default_limit: usize,
    max_limit: usize,
) -> Pagination {
    Pagination {
        limit: limit.unwrap_or(default_limit).clamp(1, max_limit),
        offset: offset.unwrap_or(0),
    }
}
