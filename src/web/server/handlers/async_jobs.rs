use crate::core::config::{Config, ConfigOverrides, RenderMode};
use crate::jobs::backend::JobKind;
use crate::services;
use crate::services::context::ServiceContext;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::state::AppState;
use super::jobs::job_lifecycle_router;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct CrawlStartRequest {
    urls: Vec<String>,
    max_pages: Option<u32>,
    max_depth: Option<usize>,
    include_subdomains: Option<bool>,
    respect_robots: Option<bool>,
    discover_sitemaps: Option<bool>,
    sitemap_since_days: Option<u32>,
    render_mode: Option<String>,
    delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct EmbedStartRequest {
    input: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ExtractStartRequest {
    urls: Vec<String>,
    prompt: Option<String>,
    max_pages: Option<u32>,
}

type IngestStartRequest = crate::mcp::schema::IngestRequest;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AcceptedJob {
    job_id: String,
    status: &'static str,
    status_url: String,
}

type WebState = (AppState, Arc<Config>);

pub(crate) fn crawl_router(service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new()
        .route("/", post(start_crawl))
        .merge(job_lifecycle_router::<WebState>(
            service_context,
            JobKind::Crawl,
        ))
}

pub(crate) fn embed_router(service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new()
        .route("/", post(start_embed))
        .merge(job_lifecycle_router::<WebState>(
            service_context,
            JobKind::Embed,
        ))
}

pub(crate) fn extract_router(service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new()
        .route("/", post(start_extract))
        .merge(job_lifecycle_router::<WebState>(
            service_context,
            JobKind::Extract,
        ))
}

pub(crate) fn ingest_router(service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new()
        .route("/", post(start_ingest))
        .merge(job_lifecycle_router::<WebState>(
            service_context,
            JobKind::Ingest,
        ))
}

#[utoipa::path(
    post,
    path = "/v1/crawl",
    request_body = CrawlStartRequest,
    responses(
        (status = 202, description = "Crawl job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid crawl request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream crawl service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_crawl(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<CrawlStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if req.urls.is_empty() {
        return Err(HttpError::bad_request("urls cannot be empty"));
    }
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        render_mode: req
            .render_mode
            .as_deref()
            .map(parse_render_mode)
            .transpose()?,
        delay_ms: req.delay_ms,
        ..ConfigOverrides::default()
    });
    let outcome =
        services::crawl::crawl_start_with_context(&cfg, &req.urls, &state.service_context, None)
            .await
            .map_err(HttpError::from_box)?;
    let Some(job_id) = outcome.result.job_ids.first().cloned() else {
        return Err(HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "crawl service returned no job id",
        ));
    };
    accepted_job("/v1/crawl", job_id)
}

#[utoipa::path(
    post,
    path = "/v1/embed",
    request_body = EmbedStartRequest,
    responses(
        (status = 202, description = "Embed job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid embed request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream embedding service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_embed(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<EmbedStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let input = super::rag::required_text(&req.input, "input")?;
    let outcome =
        services::embed::embed_start_with_context(&cfg, input, &state.service_context, None, None)
            .await
            .map_err(HttpError::from_box)?;
    accepted_job("/v1/embed", outcome.result.job_id)
}

#[utoipa::path(
    post,
    path = "/v1/extract",
    request_body = ExtractStartRequest,
    responses(
        (status = 202, description = "Extract job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid extract request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream extract service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_extract(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<ExtractStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if req.urls.is_empty() {
        return Err(HttpError::bad_request("urls cannot be empty"));
    }
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        query: Some(req.prompt.clone()),
        max_pages: req.max_pages,
        ..ConfigOverrides::default()
    });
    let outcome = services::extract::extract_start_with_context(
        &cfg,
        &req.urls,
        req.prompt,
        &state.service_context,
        None,
    )
    .await
    .map_err(HttpError::from_box)?;
    accepted_job("/v1/extract", outcome.result.job_id)
}

#[utoipa::path(
    post,
    path = "/v1/ingest",
    request_body = serde_json::Value,
    responses(
        (status = 202, description = "Ingest job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid ingest request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream ingest service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_ingest(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<IngestStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let source = ingest_source(req, &cfg)?;
    let outcome = services::ingest::ingest_start_with_context(&cfg, source, &state.service_context)
        .await
        .map_err(HttpError::from_box)?;
    accepted_job("/v1/ingest", outcome.result.job_id)
}

fn accepted_job(base: &str, job_id: String) -> Result<impl IntoResponse, HttpError> {
    let status_url = format!("{base}/{job_id}");
    Ok((
        StatusCode::ACCEPTED,
        [(header::LOCATION, status_url.clone())],
        Json(AcceptedJob {
            job_id,
            status: "pending",
            status_url,
        }),
    ))
}

fn parse_render_mode(value: &str) -> Result<RenderMode, HttpError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "http" => Ok(RenderMode::Http),
        "chrome" => Ok(RenderMode::Chrome),
        "auto-switch" | "autoswitch" | "auto" => Ok(RenderMode::AutoSwitch),
        _ => Err(HttpError::bad_request(
            "render_mode must be one of: http, chrome, auto-switch",
        )),
    }
}

fn ingest_source(
    req: IngestStartRequest,
    cfg: &Config,
) -> Result<services::ingest::IngestSource, HttpError> {
    services::ingest::source_from_mcp_request(&req, cfg).map_err(HttpError::bad_request)
}
