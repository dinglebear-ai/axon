//! Family 1: GET routes for read-only system surfaces.
//!
//! These routes call the matching `services::system::*` entry points and
//! return their existing payloads as JSON. They all require `axon:read`.

use super::error::map_service_error;
use super::state::RestState;
use crate::services::system;
use crate::services::types::Pagination;
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 1000;
const DOMAIN_SOURCES_MAX_LIMIT: usize = 10_000;

#[derive(Deserialize, Default)]
pub(crate) struct PageParams {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
}

fn to_pagination(p: PageParams) -> Pagination {
    to_pagination_with_max(p, MAX_LIMIT)
}

fn to_pagination_with_max(p: PageParams, max_limit: usize) -> Pagination {
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, max_limit);
    let offset = p.offset.unwrap_or(0);
    Pagination { limit, offset }
}

pub(crate) async fn v1_sources(
    State(state): State<RestState>,
    Query(params): Query<PageParams>,
) -> Response {
    let domain = params.domain.clone();
    let cursor = params.cursor.clone();
    if let Some(domain) = domain.as_deref() {
        let pagination = to_pagination_with_max(params, DOMAIN_SOURCES_MAX_LIMIT);
        return match system::sources_for_domain(
            state.cfg.as_ref(),
            domain,
            pagination,
            cursor.as_deref(),
        )
        .await
        {
            Ok(result) => Json(result).into_response(),
            Err(err) => map_service_error(err.as_ref()),
        };
    }
    let pagination = to_pagination(params);
    match system::sources(state.cfg.as_ref(), pagination).await {
        // Wire format intentionally matches the MCP `handle_sources` payload:
        // urls are emitted as a flat array of strings, without chunk counts.
        // Clients that need chunk counts should use the MCP `sources` action
        // until a wider REST sources response redesign happens. Keep these two
        // surfaces shape-aligned.
        Ok(result) => Json(serde_json::json!({
            "count": result.count,
            "limit": result.limit,
            "offset": result.offset,
            "urls": result.urls.iter().map(|(url, _)| url).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_domains(
    State(state): State<RestState>,
    Query(params): Query<PageParams>,
) -> Response {
    let domain = params.domain.clone();
    let pagination = to_pagination(params);
    if let Some(domain) = domain.as_deref() {
        return match system::domain_indexed(state.cfg.as_ref(), domain).await {
            Ok(result) => Json(serde_json::json!({
                "domain": result.domain,
                "indexed": result.indexed,
            }))
            .into_response(),
            Err(err) => map_service_error(err.as_ref()),
        };
    }
    match system::domains(state.cfg.as_ref(), pagination).await {
        Ok(result) => Json(serde_json::json!({
            "limit": result.limit,
            "offset": result.offset,
            "domains": result.domains.iter().map(|d| serde_json::json!({
                "domain": d.domain,
                "vectors": d.vectors,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_stats(State(state): State<RestState>) -> Response {
    match system::stats(state.cfg.as_ref()).await {
        Ok(result) => Json::<Value>(result.payload).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_doctor(State(state): State<RestState>) -> Response {
    match system::doctor(state.cfg.as_ref()).await {
        Ok(result) => Json::<Value>(result.payload).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

pub(crate) async fn v1_status(State(state): State<RestState>) -> Response {
    let ctx = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => return map_service_error(err.as_ref()),
    };
    match system::full_status(&ctx).await {
        Ok(result) => Json::<Value>(result.payload).into_response(),
        Err(err) => map_service_error(err.as_ref()),
    }
}

#[cfg(test)]
mod tests {
    use super::{DOMAIN_SOURCES_MAX_LIMIT, PageParams, to_pagination, to_pagination_with_max};

    #[test]
    fn read_only_pagination_keeps_legacy_cap() {
        let pagination = to_pagination(PageParams {
            limit: Some(10_000),
            offset: Some(2),
            domain: None,
            cursor: None,
        });

        assert_eq!(pagination.limit, 1000);
        assert_eq!(pagination.offset, 2);
    }

    #[test]
    fn read_only_domain_sources_pagination_allows_export_cap() {
        let pagination = to_pagination_with_max(
            PageParams {
                limit: Some(10_000),
                offset: Some(0),
                domain: Some("example.com".to_string()),
                cursor: None,
            },
            DOMAIN_SOURCES_MAX_LIMIT,
        );

        assert_eq!(pagination.limit, 10_000);
        assert_eq!(pagination.offset, 0);
    }
}
