//! Family 1: GET routes for read-only system surfaces.
//!
//! These routes call the matching `services::system::*` entry points and
//! return their existing payloads as JSON. They all require `axon:read`.

use super::error::map_service_error;
use super::state::RestState;
use axon_services::system;
use axon_services::transport;
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;

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

fn to_pagination(p: PageParams) -> axon_services::types::Pagination {
    transport::discovery_pagination(p.limit, p.offset)
}

fn to_domain_sources_pagination(p: PageParams) -> axon_services::types::Pagination {
    transport::domain_sources_pagination(p.limit, p.offset)
}

pub(crate) async fn v1_sources(
    State(state): State<RestState>,
    Query(params): Query<PageParams>,
) -> Response {
    let domain = params.domain.clone();
    let cursor = params.cursor.clone();
    if let Some(domain) = domain.as_deref() {
        let pagination = to_domain_sources_pagination(params);
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
        Ok(result) => Json(result).into_response(),
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
    use super::{PageParams, to_domain_sources_pagination, to_pagination};
    use axon_services::transport;

    #[test]
    fn read_only_pagination_uses_shared_discovery_cap() {
        let pagination = to_pagination(PageParams {
            limit: Some(10_000),
            offset: Some(2),
            domain: None,
            cursor: None,
        });

        assert_eq!(pagination.limit, transport::PAGE_LIMIT_MAX);
        assert_eq!(pagination.offset, 2);
    }

    #[test]
    fn read_only_domain_sources_pagination_allows_export_cap() {
        let pagination = to_domain_sources_pagination(PageParams {
            limit: Some(10_000),
            offset: Some(0),
            domain: Some("example.com".to_string()),
            cursor: None,
        });

        assert_eq!(pagination.limit, 10_000);
        assert_eq!(pagination.offset, 0);
    }
}
