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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub(crate) struct EmbedStartRequest {
    input: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtractStartRequest {
    urls: Vec<String>,
    prompt: Option<String>,
    max_pages: Option<u32>,
}

type IngestStartRequest = crate::mcp::schema::IngestRequest;

#[derive(Debug, Serialize)]
struct AcceptedJob {
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

/// Validate URLs for SSRF before enqueue — rejects private-IP targets with
/// a 400 so callers learn immediately rather than after a worker run.
fn validate_ssrf_urls(urls: &[String]) -> Result<(), HttpError> {
    for url in urls {
        crate::core::http::validate_url(url)
            .map_err(|e| HttpError::bad_request(format!("{url}: {e}").as_str()))?;
    }
    Ok(())
}

async fn start_crawl(
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

/// Guard embed input against path traversal and symlinks — mirrors MCP's
/// `validate_mcp_embed_input_with_roots`. Runs blocking I/O in a spawn_blocking
/// task so the async handler does not block the tokio executor.
async fn validate_embed_path(input: &str) -> Result<(), HttpError> {
    let input = input.trim().to_string();
    tokio::task::spawn_blocking(move || validate_embed_path_sync(&input))
        .await
        .map_err(|e| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?
}

fn validate_embed_path_sync(input: &str) -> Result<(), HttpError> {
    if input.starts_with("http://") || input.starts_with("https://") {
        return crate::core::http::validate_url(input)
            .map_err(|e| HttpError::bad_request(e.to_string().as_str()));
    }
    let path = std::path::Path::new(input);
    if !path.exists() {
        return Ok(());
    }
    if std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(HttpError::bad_request(
            "local embed path must not be a symlink",
        ));
    }
    let allowed_roots: Vec<std::path::PathBuf> = std::env::var("AXON_MCP_EMBED_ALLOWED_ROOTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|p| {
                    let t = p.trim();
                    (!t.is_empty()).then(|| std::path::PathBuf::from(t))
                })
                .collect()
        })
        .unwrap_or_default();
    if allowed_roots.is_empty() {
        return Err(HttpError::bad_request(
            "local file embedding is disabled; set AXON_MCP_EMBED_ALLOWED_ROOTS to allow specific roots",
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| HttpError::bad_request(format!("invalid embed path: {e}").as_str()))?;
    let root = allowed_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| canonical.starts_with(root))
        .ok_or_else(|| {
            HttpError::bad_request(
                "local embed path must be under one of AXON_MCP_EMBED_ALLOWED_ROOTS",
            )
        })?;
    if let Ok(relative) = canonical.strip_prefix(&root) {
        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy();
            if name.starts_with('.') {
                return Err(HttpError::bad_request(
                    "local embed path must not include dotfiles",
                ));
            }
        }
    }
    Ok(())
}

async fn start_embed(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<EmbedStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let input = super::rag::required_text(&req.input, "input")?;
    validate_embed_path(input).await?;
    let outcome =
        services::embed::embed_start_with_context(&cfg, input, &state.service_context, None, None)
            .await
            .map_err(HttpError::from_box)?;
    accepted_job("/v1/embed", outcome.result.job_id)
}

async fn start_extract(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<ExtractStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if req.urls.is_empty() {
        return Err(HttpError::bad_request("urls cannot be empty"));
    }
    validate_ssrf_urls(&req.urls)?;
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

async fn start_ingest(
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
