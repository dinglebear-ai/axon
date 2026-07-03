use axon_core::config::{Config, ConfigOverrides};
use axon_services as services;
use axon_services::client_contract::{
    RestEvaluateRequest as EvaluateRequest, RestQueryRequest as QueryRequest,
    RestRetrieveRequest as RetrieveRequest, RestSuggestRequest as SuggestRequest,
};
use axon_services::transport;
use axum::{Json, extract::State};
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[utoipa::path(
    post,
    path = "/v1/query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Semantic query results", body = serde_json::Value),
        (status = 400, description = "Invalid query request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn query(
    State((state, cfg)): State<WebState>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<services::types::QueryResult>, HttpError> {
    let query = required_text(&req.query, "query")?;
    let cfg = with_query_overrides(
        &cfg,
        req.collection,
        req.since,
        req.before,
        req.hybrid_search,
    )?;
    services::query::query(
        &state.service_context,
        &cfg,
        query,
        transport::pagination(req.limit, req.offset, cfg.search_limit),
    )
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
        (status = 400, description = "Invalid retrieve request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn retrieve(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<RetrieveRequest>,
) -> Result<Json<services::types::RetrieveResult>, HttpError> {
    let url = required_text(&req.url, "url")?;
    let cfg = with_query_overrides(&cfg, req.collection, req.since, req.before, None)?;
    services::query::retrieve(
        &cfg,
        url,
        transport::retrieve_options(req.max_points, req.cursor, req.token_budget),
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
        (status = 400, description = "Invalid evaluation request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream LLM or vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub(crate) async fn evaluate(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<services::types::EvaluateResult>, HttpError> {
    let question = required_text(&req.question, "question")?;
    let mut cfg = with_query_overrides(
        &cfg,
        req.collection,
        req.since,
        req.before,
        req.hybrid_search,
    )?;
    if let Some(diagnostics) = req.diagnostics {
        cfg.ask_diagnostics = diagnostics;
    }
    if let Some(retrieval_ab) = req.retrieval_ab {
        cfg.evaluate_retrieval_ab = retrieval_ab;
    }
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
        (status = 429, description = "Upstream LLM or search provider rate limited", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream search or LLM service unavailable", body = crate::server::error::ErrorBody)
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
    let cfg = with_collection_override(&cfg, req.collection)?;
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

pub(crate) fn with_collection_override(
    cfg: &Config,
    collection: Option<String>,
) -> Result<Config, HttpError> {
    let Some(collection) = collection
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(cfg.clone());
    };
    validate_collection_name(collection)?;
    let mut cfg = cfg.clone();
    cfg.collection = collection.to_string();
    Ok(cfg)
}

pub(crate) fn with_query_overrides(
    cfg: &Config,
    collection: Option<String>,
    since: Option<String>,
    before: Option<String>,
    hybrid_search: Option<bool>,
) -> Result<Config, HttpError> {
    let collection = if let Some(collection) = collection
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        validate_collection_name(collection)?;
        Some(collection.to_string())
    } else {
        None
    };
    Ok(cfg.apply_overrides(&ConfigOverrides {
        collection,
        since,
        before,
        hybrid_search_enabled: hybrid_search,
        ..ConfigOverrides::default()
    }))
}

pub(crate) fn validate_collection_name(collection: &str) -> Result<(), HttpError> {
    axon_core::config::validation::validate_collection_name(collection)
        .map_err(|err| HttpError::bad_request(format!("invalid collection: {err}")))
}
