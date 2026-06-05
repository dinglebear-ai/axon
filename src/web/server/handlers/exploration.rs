use crate::core::config::{Config, ConfigOverrides};
use crate::core::http::{normalize_url, validate_url};
use crate::services;
use crate::services::client_contract::{
    RestMapRequest as MapRequest, RestResearchRequest as ResearchRequest,
    RestScrapeRequest as ScrapeRequest, RestSearchRequest as SearchRequest,
    RestSummarizeRequest as SummarizeRequest,
};
use crate::services::events::ServiceEvent;
use crate::services::types::{MapOptions, SearchOptions, ServiceTimeRange};
use axum::response::{
    IntoResponse, Response,
    sse::{Event, Sse},
};
use axum::{Json, extract::State, http::StatusCode};
use futures_util::stream;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

use super::super::error::HttpError;
use super::rag::required_text;

type WebState = (super::super::state::AppState, Arc<Config>);

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LlmStreamEvent<T: Serialize> {
    Meta { phase: &'static str },
    Delta { text: String },
    Done { result: T },
    Error { message: String },
}

fn sse_json<T: Serialize>(event_name: &'static str, value: &LlmStreamEvent<T>) -> Event {
    Event::default()
        .event(event_name)
        .json_data(value)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("{\"type\":\"error\",\"message\":\"encode failed\"}")
        })
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct EndpointsRequest {
    url: String,
    include_bundles: Option<bool>,
    first_party_only: Option<bool>,
    unique_only: Option<bool>,
    max_scripts: Option<usize>,
    max_scan_bytes: Option<usize>,
    verify: Option<bool>,
    capture_network: Option<bool>,
    probe_rpc: Option<bool>,
    probe_rpc_subdomains: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct BrandRequest {
    url: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct DiffRequest {
    url_a: String,
    url_b: String,
    render_mode: Option<crate::core::config::RenderMode>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ScreenshotRequest {
    url: String,
    viewport: Option<String>,
    full_page: Option<bool>,
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
    let urls = request_urls(&req)?;
    let cfg = scrape_config(&cfg, &req)?;
    let results = services::scrape::scrape_batch_with_optional_embed(&cfg, &urls, None)
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
    let urls = summarize_request_urls(&req)?;
    let cfg = summarize_config(&cfg, &req);
    services::summarize::summarize(&cfg, &urls, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/summarize/stream",
    request_body = SummarizeRequest,
    responses(
        (status = 200, description = "Brief LLM summary streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid summarize request", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn summarize_stream(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<SummarizeRequest>,
) -> Response {
    let urls = match summarize_request_urls(&req) {
        Ok(urls) => urls,
        Err(err) => return err.into_response(),
    };
    let cfg = summarize_config(&cfg, &req);
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));

    tokio::spawn(async move {
        let _ = tx.send(Ok(sse_json(
            "meta",
            &LlmStreamEvent::<services::types::SummarizeResult>::Meta {
                phase: "summarizing",
            },
        )));
        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_tx = tx.clone();
        let delta_disconnected = Arc::clone(&disconnected);
        let delta_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if delta_disconnected.load(Ordering::Relaxed) {
                    return;
                }
                if let ServiceEvent::SynthesisDelta { text } = event
                    && delta_tx
                        .send(Ok(sse_json(
                            "delta",
                            &LlmStreamEvent::<services::types::SummarizeResult>::Delta { text },
                        )))
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
        let result = services::summarize::summarize(&cfg, &urls, Some(event_tx))
            .await
            .map_err(|err| err.to_string());
        let _ = delta_task.await;
        if disconnected.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(result) => {
                let _ = tx.send(Ok(sse_json("done", &LlmStreamEvent::Done { result })));
            }
            Err(message) => {
                let _ = tx.send(Ok(sse_json(
                    "error",
                    &LlmStreamEvent::<services::types::SummarizeResult>::Error { message },
                )));
            }
        }
    });

    let event_stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| (event, rx))
    });
    Sse::new(event_stream).into_response()
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
    let url = normalize_url(url);
    services::map::discover(&cfg, &url, map_options(req.limit, req.offset), None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/endpoints",
    request_body = EndpointsRequest,
    responses(
        (status = 200, description = "Discovered endpoint report", body = services::types::EndpointReport),
        (status = 400, description = "Invalid endpoint discovery request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream fetch or verification service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn endpoints(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<EndpointsRequest>,
) -> Result<Json<services::types::EndpointReport>, HttpError> {
    let url = required_text(&req.url, "url")?;
    validate_url(url)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, "bad_request", err.to_string()))?;
    let mut options = services::endpoints::options_from_config(&cfg);
    if let Some(value) = req.include_bundles {
        options.include_bundles = value;
    }
    if let Some(value) = req.first_party_only {
        options.first_party_only = value;
    }
    if let Some(value) = req.unique_only {
        options.unique_only = value;
    }
    if let Some(value) = req.max_scripts {
        options.max_scripts = value;
    }
    if let Some(value) = req.max_scan_bytes {
        options.max_scan_bytes = value;
    }
    if let Some(value) = req.verify {
        options.verify = value;
    }
    if let Some(value) = req.capture_network {
        options.capture_network = value;
    }
    if let Some(value) = req.probe_rpc {
        options.probe_rpc = value;
    }
    if let Some(value) = req.probe_rpc_subdomains {
        options.probe_rpc_subdomains = value;
    }
    services::endpoints::discover(&cfg, url, options, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box_send_sync)
}

#[utoipa::path(
    post,
    path = "/v1/brand",
    request_body = BrandRequest,
    responses(
        (status = 200, description = "Extracted brand identity from a URL", body = services::types::BrandResult),
        (status = 400, description = "Invalid brand request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream page fetch unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn brand(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<BrandRequest>,
) -> Result<Json<services::types::BrandResult>, HttpError> {
    let url = required_text(&req.url, "url")?;
    validate_url(url)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, "bad_request", err.to_string()))?;
    services::brand::brand(&cfg, url, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/diff",
    request_body = DiffRequest,
    responses(
        (status = 200, description = "Markdown, metadata, and link diff between two URLs", body = services::types::DiffResult),
        (status = 400, description = "Invalid diff request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream scrape unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn diff(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<DiffRequest>,
) -> Result<Json<services::types::DiffResult>, HttpError> {
    let url_a = required_text(&req.url_a, "url_a")?;
    let url_b = required_text(&req.url_b, "url_b")?;
    validate_url(url_a)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, "bad_request", err.to_string()))?;
    validate_url(url_b)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, "bad_request", err.to_string()))?;
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode,
        ..ConfigOverrides::default()
    });
    services::diff::diff(&cfg, url_a, url_b, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/screenshot",
    request_body = ScreenshotRequest,
    responses(
        (status = 200, description = "Captured screenshot artifact metadata", body = services::types::ScreenshotResult),
        (status = 400, description = "Invalid screenshot request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Chrome screenshot service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn screenshot(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<ScreenshotRequest>,
) -> Result<Json<services::types::ScreenshotResult>, HttpError> {
    let url = required_text(&req.url, "url")?;
    validate_url(url)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, "bad_request", err.to_string()))?;
    let (viewport_width, viewport_height) = parse_viewport(
        req.viewport.as_deref(),
        cfg.viewport_width,
        cfg.viewport_height,
    )?;
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        viewport_width: Some(viewport_width),
        viewport_height: Some(viewport_height),
        screenshot_full_page: req.full_page,
        ..ConfigOverrides::default()
    });
    services::screenshot::screenshot_capture(&cfg, url)
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
    State((state, cfg)): State<WebState>,
    Json(req): Json<ResearchRequest>,
) -> Result<Json<services::types::ResearchResult>, HttpError> {
    let query = required_text(&req.query, "query")?.to_string();
    let opts = search_options(req.limit, req.offset, req.time_range.as_deref())?;
    let service_context = Arc::clone(&state.service_context);
    tokio::time::timeout(
        Duration::from_secs(35),
        services::search::research_with_context(&cfg, &service_context, &query, opts, None),
    )
    .await
    .map_err(|_| HttpError::new(StatusCode::GATEWAY_TIMEOUT, "timeout", "research timed out"))?
    .map(Json)
    .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/research/stream",
    request_body = ResearchRequest,
    responses(
        (status = 200, description = "Research synthesis streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid research request", body = crate::web::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn research_stream(
    State((state, cfg)): State<WebState>,
    Json(req): Json<ResearchRequest>,
) -> Response {
    let query = match required_text(&req.query, "query") {
        Ok(query) => query.to_string(),
        Err(err) => return err.into_response(),
    };
    let opts = match search_options(req.limit, req.offset, req.time_range.as_deref()) {
        Ok(opts) => opts,
        Err(err) => return err.into_response(),
    };
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));
    let service_context = Arc::clone(&state.service_context);

    tokio::spawn(async move {
        let _ = tx.send(Ok(sse_json(
            "meta",
            &LlmStreamEvent::<services::types::ResearchPayload>::Meta {
                phase: "researching",
            },
        )));
        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_tx = tx.clone();
        let delta_disconnected = Arc::clone(&disconnected);
        let delta_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if delta_disconnected.load(Ordering::Relaxed) {
                    return;
                }
                if let ServiceEvent::SynthesisDelta { text } = event
                    && delta_tx
                        .send(Ok(sse_json(
                            "delta",
                            &LlmStreamEvent::<services::types::ResearchPayload>::Delta { text },
                        )))
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
        let result = services::search::research_with_context(
            &cfg,
            &service_context,
            &query,
            opts,
            Some(event_tx),
        )
        .await
        .map(|result| result.payload)
        .map_err(|err| err.to_string());
        let _ = delta_task.await;
        if disconnected.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(result) => {
                let _ = tx.send(Ok(sse_json("done", &LlmStreamEvent::Done { result })));
            }
            Err(message) => {
                let _ = tx.send(Ok(sse_json(
                    "error",
                    &LlmStreamEvent::<services::types::ResearchPayload>::Error { message },
                )));
            }
        }
    });

    let event_stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| (event, rx))
    });
    Sse::new(event_stream).into_response()
}

fn request_urls(req: &ScrapeRequest) -> Result<Vec<String>, HttpError> {
    let urls: Vec<String> = req
        .urls
        .clone()
        .unwrap_or_default()
        .into_iter()
        .chain(req.url.clone())
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

fn scrape_config(cfg: &Config, req: &ScrapeRequest) -> Result<Config, HttpError> {
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode,
        format: req.format,
        embed: req.embed,
        collection: req.collection.clone(),
        root_selector: req.root_selector.clone(),
        exclude_selector: req.exclude_selector.clone(),
        custom_headers: if req.headers.is_empty() {
            None
        } else {
            Some(req.headers.clone())
        },
        ..ConfigOverrides::default()
    });
    super::rag::validate_collection_name(&cfg.collection)?;
    Ok(cfg)
}

fn summarize_request_urls(req: &SummarizeRequest) -> Result<Vec<String>, HttpError> {
    let urls: Vec<String> = req
        .urls
        .clone()
        .unwrap_or_default()
        .into_iter()
        .chain(req.url.clone())
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .collect();
    if urls.is_empty() {
        return Err(HttpError::bad_request("url or urls is required"));
    }
    Ok(urls)
}

fn summarize_config(cfg: &Config, req: &SummarizeRequest) -> Config {
    cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode,
        root_selector: req.root_selector.clone(),
        exclude_selector: req.exclude_selector.clone(),
        custom_headers: if req.headers.is_empty() {
            None
        } else {
            Some(req.headers.clone())
        },
        ..ConfigOverrides::default()
    })
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

fn parse_viewport(
    viewport: Option<&str>,
    fallback_w: u32,
    fallback_h: u32,
) -> Result<(u32, u32), HttpError> {
    let Some(value) = viewport.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((fallback_w, fallback_h));
    };
    let Some((w, h)) = value.split_once('x') else {
        return Err(HttpError::bad_request(
            "viewport must use WxH format, for example 1280x720",
        ));
    };
    let w = w
        .parse::<u32>()
        .map_err(|_| HttpError::bad_request("viewport width must be a positive integer"))?;
    let h = h
        .parse::<u32>()
        .map_err(|_| HttpError::bad_request("viewport height must be a positive integer"))?;
    if w == 0 || h == 0 || w > 7680 || h > 4320 {
        return Err(HttpError::bad_request(
            "viewport must be between 1x1 and 7680x4320",
        ));
    }
    Ok((w, h))
}
