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

#[derive(Debug, Deserialize)]
pub(crate) struct ScrapeRequest {
    url: Option<String>,
    urls: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MapRequest {
    url: String,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchRequest {
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResearchRequest {
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<String>,
}

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
