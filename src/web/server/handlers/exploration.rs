use crate::core::config::Config;
use crate::services;
use crate::services::types::{MapOptions, SearchOptions, ServiceTimeRange};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use super::super::error::HttpError;
use super::rag::required_text;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ScrapeRequest {
    url: Option<String>,
    urls: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SummarizeRequest {
    url: Option<String>,
    urls: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MapRequest {
    url: String,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SearchRequest {
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ResearchRequest {
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/scrape",
    request_body = ScrapeRequest,
    responses(
        (status = 200, description = "Scraped document or batch scrape results", body = serde_json::Value),
        (status = 400, description = "Invalid scrape request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream crawl or render service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn scrape(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<ScrapeRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let urls = request_urls(req)?;
    let results = services::scrape::scrape_batch(&cfg, &urls, None)
        .await
        .map_err(HttpError::from_box)?;
    if results.len() == 1 {
        Ok(Json(serde_json::to_value(&results[0]).map_err(|err| {
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                err.to_string(),
            )
        })?))
    } else {
        Ok(Json(json!({ "results": results })))
    }
}

#[utoipa::path(
    post,
    path = "/v1/summarize",
    request_body = SummarizeRequest,
    responses(
        (status = 200, description = "Brief LLM summary of scraped URL content", body = serde_json::Value),
        (status = 400, description = "Invalid summarize request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream crawl, render, or LLM service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn summarize(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<SummarizeRequest>,
) -> Result<Json<services::types::SummarizeResult>, HttpError> {
    let urls = summarize_request_urls(req)?;
    services::summarize::summarize(&cfg, &urls, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/map",
    request_body = MapRequest,
    responses(
        (status = 200, description = "Discovered URLs", body = serde_json::Value),
        (status = 400, description = "Invalid map request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream crawl service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn map(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<MapRequest>,
) -> Result<Json<services::types::MapResult>, HttpError> {
    let url = required_text(&req.url, "url")?;
    services::map::discover(&cfg, url, map_options(req.limit, req.offset), None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results and queued crawl jobs", body = serde_json::Value),
        (status = 400, description = "Invalid search request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream search service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn search(
    State((state, cfg)): State<WebState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let query = required_text(&req.query, "query")?;
    let result = services::search_crawl::search_and_crawl(
        &cfg,
        &state.service_context,
        query,
        search_options(req.limit, req.offset, req.time_range.as_deref())?,
    )
    .await
    .map_err(HttpError::from_box)?;
    Ok(Json(json!({
        "results": result.results,
        "crawl_jobs": result.crawl_jobs,
        "crawl_rejected": result.crawl_rejected,
        "auto_crawl_status": result.auto_crawl_status,
    })))
}

#[utoipa::path(
    post,
    path = "/v1/research",
    request_body = ResearchRequest,
    responses(
        (status = 200, description = "Research synthesis", body = serde_json::Value),
        (status = 400, description = "Invalid research request", body = crate::web::server::error::ErrorBody),
        (status = 504, description = "Research request timed out", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn research(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<ResearchRequest>,
) -> Result<Json<services::types::ResearchResult>, HttpError> {
    let query = required_text(&req.query, "query")?.to_string();
    let opts = search_options(req.limit, req.offset, req.time_range.as_deref())?;
    tokio::time::timeout(
        Duration::from_secs(35),
        services::search::research(&cfg, &query, opts, None),
    )
    .await
    .map_err(|_| HttpError::new(StatusCode::GATEWAY_TIMEOUT, "timeout", "research timed out"))?
    .map(Json)
    .map_err(HttpError::from_box)
}

fn request_urls(req: ScrapeRequest) -> Result<Vec<String>, HttpError> {
    let urls: Vec<String> = req
        .urls
        .unwrap_or_default()
        .into_iter()
        .chain(req.url)
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .collect();
    if urls.is_empty() {
        return Err(HttpError::bad_request("url or urls is required"));
    }
    let mut seen = HashSet::new();
    Ok(urls
        .into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect())
}

fn summarize_request_urls(req: SummarizeRequest) -> Result<Vec<String>, HttpError> {
    let urls: Vec<String> = req
        .urls
        .unwrap_or_default()
        .into_iter()
        .chain(req.url)
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .collect();
    if urls.is_empty() {
        return Err(HttpError::bad_request("url or urls is required"));
    }
    Ok(urls)
}

fn map_options(limit: Option<usize>, offset: Option<usize>) -> MapOptions {
    MapOptions {
        limit: limit.unwrap_or(0),
        offset: offset.unwrap_or(0),
    }
}

fn search_options(
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<&str>,
) -> Result<SearchOptions, HttpError> {
    Ok(SearchOptions {
        limit: limit.unwrap_or(10).clamp(1, 100),
        offset: offset.unwrap_or(0),
        time_range: time_range.map(parse_time_range).transpose()?,
    })
}

fn parse_time_range(value: &str) -> Result<ServiceTimeRange, HttpError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "day" => Ok(ServiceTimeRange::Day),
        "week" => Ok(ServiceTimeRange::Week),
        "month" => Ok(ServiceTimeRange::Month),
        "year" => Ok(ServiceTimeRange::Year),
        _ => Err(HttpError::bad_request(
            "time_range must be one of: day, week, month, year",
        )),
    }
}
