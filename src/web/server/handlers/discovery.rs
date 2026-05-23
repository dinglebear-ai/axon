use crate::core::config::Config;
use crate::services;
use crate::services::types::Pagination;
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(crate) struct PaginationQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    domain: Option<String>,
    cursor: Option<String>,
}

impl PaginationQuery {
    pub(crate) fn pagination(&self, default_limit: usize) -> Pagination {
        Pagination {
            limit: self.limit.unwrap_or(default_limit).clamp(1, 500),
            offset: self.offset.unwrap_or(0),
        }
    }
}

#[utoipa::path(
    get,
    path = "/v1/sources",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Indexed sources", body = serde_json::Value),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(crate) async fn sources(
    State((_state, cfg)): State<WebState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    if let Some(domain) = query.domain.as_deref() {
        return services::system::sources_for_domain(
            &cfg,
            domain,
            query.pagination(100),
            query.cursor.as_deref(),
        )
        .await
        .and_then(|result| {
            serde_json::to_value(result).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
        })
        .map(Json)
        .map_err(HttpError::from_box);
    }
    services::system::sources(&cfg, query.pagination(100))
        .await
        .and_then(|result| {
            serde_json::to_value(result).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
        })
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/domains",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Indexed domains", body = serde_json::Value),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(crate) async fn domains(
    State((_state, cfg)): State<WebState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    if let Some(domain) = query.domain.as_deref() {
        return services::system::domain_indexed(&cfg, domain)
            .await
            .and_then(|result| {
                serde_json::to_value(result)
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
            })
            .map(Json)
            .map_err(HttpError::from_box);
    }
    services::system::domains(&cfg, query.pagination(100))
        .await
        .and_then(|result| {
            serde_json::to_value(result).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
        })
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/stats",
    responses(
        (status = 200, description = "Collection statistics", body = serde_json::Value),
        (status = 502, description = "Upstream vector service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(crate) async fn stats(
    State((_state, cfg)): State<WebState>,
) -> Result<Json<services::types::StatsResult>, HttpError> {
    services::system::stats(&cfg)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/status",
    responses(
        (status = 200, description = "Job queue status", body = serde_json::Value),
        (status = 502, description = "Job storage unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(crate) async fn status(
    State((state, _cfg)): State<WebState>,
) -> Result<Json<services::types::StatusResult>, HttpError> {
    services::system::full_status(&state.service_context)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    get,
    path = "/v1/doctor",
    responses(
        (status = 200, description = "Service health diagnostics", body = serde_json::Value),
        (status = 502, description = "Health check failed", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(crate) async fn doctor(
    State((_state, cfg)): State<WebState>,
) -> Result<Json<services::types::DoctorResult>, HttpError> {
    services::system::doctor(&cfg)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}
