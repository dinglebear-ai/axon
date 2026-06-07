use crate::core::config::{Config, ConfigOverrides};
use crate::jobs::backend::JobKind;
use crate::services;
use crate::services::client_contract::{
    RestCrawlRequest as CrawlStartRequest, RestEmbedRequest as EmbedStartRequest, RestExtractMode,
    RestExtractRequest as ExtractStartRequest, RestIngestRequest as IngestStartRequest,
};
use crate::services::context::ServiceContext;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde::Serialize;
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::state::AppState;
use super::jobs::job_lifecycle_router;

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

pub(crate) fn prepared_sessions_router(_service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new().route("/", post(start_prepared_sessions_ingest))
}

/// Validate URLs for SSRF before enqueue — rejects private-IP targets with
/// a 400 so callers learn immediately rather than after a worker run.
fn validate_ssrf_urls(urls: &[String]) -> Result<(), HttpError> {
    for url in urls {
        crate::core::http::validate_url(url)
            .map_err(|e| HttpError::bad_request(format!("{url}: {e}").as_str()))?;
    }
    Ok(())
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
    validate_ssrf_urls(&req.urls)?;
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        max_sitemaps: req.max_sitemaps,
        discover_llms_txt: req.discover_llms_txt,
        max_llms_txt_urls: req.max_llms_txt_urls,
        render_mode: req.render_mode,
        delay_ms: req.delay_ms,
        collection: req.collection,
        custom_headers: if req.headers.is_empty() {
            None
        } else {
            Some(req.headers)
        },
        ..ConfigOverrides::default()
    });
    super::rag::validate_collection_name(&cfg.collection)?;
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

/// Guard embed input against path traversal, secret-like paths, and symlinks.
async fn validate_embed_path(cfg: &Config, input: &str) -> Result<String, HttpError> {
    let cfg = cfg.clone();
    let input = input.trim().to_string();
    tokio::task::spawn_blocking(move || {
        services::embed::validate_server_embed_input_with_config(&cfg, &input)
    })
    .await
    .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?
    .map_err(|err| HttpError::bad_request(err.to_string()))
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
    let input = validate_embed_path(state.service_context.cfg.as_ref(), input).await?;
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        collection: req.collection,
        ..ConfigOverrides::default()
    });
    super::rag::validate_collection_name(&cfg.collection)?;
    let outcome = services::embed::embed_start_with_context(
        &cfg,
        &input,
        &state.service_context,
        None,
        req.source_type.as_deref(),
    )
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
    if !matches!(
        req.mode.unwrap_or(RestExtractMode::Auto),
        RestExtractMode::Auto
    ) {
        return Err(HttpError::bad_request(
            "extract mode overrides are not supported by the REST job API yet",
        ));
    }
    validate_ssrf_urls(&req.urls)?;
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        query: Some(req.prompt.clone()),
        max_pages: req.max_pages,
        render_mode: req.render_mode,
        embed: req.embed,
        collection: req.collection,
        custom_headers: if req.headers.is_empty() {
            None
        } else {
            Some(req.headers)
        },
        ..ConfigOverrides::default()
    });
    super::rag::validate_collection_name(&cfg.collection)?;
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
    request_body = IngestStartRequest,
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

#[utoipa::path(
    post,
    path = "/v1/ingest/sessions/prepared",
    request_body = crate::ingest::sessions::IngestSessionsPreparedRequest,
    responses(
        (status = 202, description = "Prepared sessions ingest job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid prepared sessions request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream ingest service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_prepared_sessions_ingest(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<crate::ingest::sessions::IngestSessionsPreparedRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let outcome = services::ingest::ingest_sessions_prepared_start_with_context(
        &cfg,
        req,
        &state.service_context,
    )
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

fn ingest_source(
    req: IngestStartRequest,
    cfg: &Config,
) -> Result<services::ingest::IngestSource, HttpError> {
    let req = crate::mcp::schema::IngestRequest::from(req);
    services::ingest::source_from_mcp_request(&req, cfg).map_err(HttpError::bad_request)
}
