use crate::core::config::{Config, ConfigOverrides, RenderMode, ScrapeFormat};
use crate::jobs::backend::JobKind;
use crate::mcp::schema::{
    CrawlRequest, CrawlSubaction, EmbedRequest, EmbedSubaction, ExtractRequest, ExtractSubaction,
    IngestRequest, IngestSourceType, IngestSubaction, McpRenderMode, McpScrapeFormat,
    ScrapeRequest, ScreenshotRequest,
};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_svc;
use crate::services::embed as embed_svc;
use crate::services::extract as extract_svc;
use crate::services::ingest as ingest_svc;
use crate::services::jobs as job_svc;
use crate::services::scrape as scrape_svc;
use crate::services::screenshot as screenshot_svc;
use crate::services::types::ClientActionError;
use uuid::Uuid;

use super::internal_error;

pub(super) async fn dispatch_crawl(
    service_context: &ServiceContext,
    req: CrawlRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let subaction = match req.subaction {
        Some(subaction) => subaction,
        None => CrawlSubaction::Start,
    };
    match subaction {
        CrawlSubaction::Start => {
            let urls = req.urls.clone().ok_or_else(|| {
                ClientActionError::new(
                    "invalid_request",
                    "urls are required for crawl.start",
                    false,
                    None,
                )
            })?;
            if urls.is_empty() {
                return Err(ClientActionError::new(
                    "invalid_request",
                    "urls cannot be empty",
                    false,
                    None,
                ));
            }
            let cfg = apply_crawl_overrides(service_context.cfg.as_ref(), &req);
            let outcome = crawl_svc::crawl_start_with_context(&cfg, &urls, service_context, None)
                .await
                .map_err(internal_error)?;
            let result = outcome.result;
            Ok(serde_json::json!({
                "job_ids": result.job_ids,
                "output_dir": result.output_dir,
                "predicted_paths": result.predicted_paths,
                "predicted_artifact_handles": result.predicted_artifact_handles,
                "jobs": result.jobs.into_iter().map(|job| serde_json::json!({
                    "job_id": job.job_id,
                    "url": job.url,
                    "output_dir": job.output_dir,
                    "predicted_paths": job.predicted_paths,
                    "predicted_artifact_handles": job.predicted_artifact_handles,
                })).collect::<Vec<_>>(),
            }))
        }
        CrawlSubaction::Status => {
            let id = parse_job_id(req.job_id.as_deref())?;
            let result = crawl_svc::crawl_status(service_context, id)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "job": result.payload,
                "output_files": result.output_files,
            }))
        }
        CrawlSubaction::List => {
            let limit = match req.limit {
                Some(limit) => limit.clamp(1, 500),
                None => 20,
            };
            let offset = match req.offset {
                Some(offset) => offset.min(i64::MAX as usize) as i64,
                None => 0,
            };
            let result = crawl_svc::crawl_list(service_context, limit, offset)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "jobs": result.payload,
                "limit": limit,
                "offset": offset,
            }))
        }
        CrawlSubaction::Cancel => {
            let id = parse_job_id(req.job_id.as_deref())?;
            let canceled = crawl_svc::crawl_cancel(service_context, id)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "job_id": id.to_string(),
                "canceled": canceled,
            }))
        }
        CrawlSubaction::Cleanup => {
            let deleted = crawl_svc::crawl_cleanup(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Clear => {
            let deleted = crawl_svc::crawl_clear(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Recover => {
            let recovered = crawl_svc::crawl_recover(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "recovered": recovered }))
        }
    }
}

pub(super) async fn dispatch_extract(
    service_context: &ServiceContext,
    req: ExtractRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(ExtractSubaction::Start) {
        ExtractSubaction::Start => {
            let urls = req.urls.ok_or_else(|| {
                ClientActionError::new("invalid_request", "urls are required", false, None)
            })?;
            let prompt = req.prompt.clone();
            let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
                query: Some(prompt.clone()),
                max_pages: req.max_pages,
                ..ConfigOverrides::default()
            });
            let outcome =
                extract_svc::extract_start_with_context(&cfg, &urls, prompt, service_context, None)
                    .await
                    .map_err(internal_error)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        ExtractSubaction::Status => job_status(service_context, JobKind::Extract, req.job_id).await,
        ExtractSubaction::Cancel => job_cancel(service_context, JobKind::Extract, req.job_id).await,
        ExtractSubaction::List => {
            job_list(service_context, JobKind::Extract, req.limit, req.offset).await
        }
        ExtractSubaction::Cleanup => job_cleanup(service_context, JobKind::Extract).await,
        ExtractSubaction::Clear => job_clear(service_context, JobKind::Extract).await,
        ExtractSubaction::Recover => job_recover(service_context, JobKind::Extract).await,
    }
}

pub(super) async fn dispatch_embed(
    service_context: &ServiceContext,
    req: EmbedRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(EmbedSubaction::Start) {
        EmbedSubaction::Start => {
            let input = req.input.ok_or_else(|| {
                ClientActionError::new("invalid_request", "input is required", false, None)
            })?;
            let outcome = embed_svc::embed_start_with_context(
                service_context.cfg.as_ref(),
                &input,
                service_context,
                None,
                None,
            )
            .await
            .map_err(internal_error)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        EmbedSubaction::Status => job_status(service_context, JobKind::Embed, req.job_id).await,
        EmbedSubaction::Cancel => job_cancel(service_context, JobKind::Embed, req.job_id).await,
        EmbedSubaction::List => {
            job_list(service_context, JobKind::Embed, req.limit, req.offset).await
        }
        EmbedSubaction::Cleanup => job_cleanup(service_context, JobKind::Embed).await,
        EmbedSubaction::Clear => job_clear(service_context, JobKind::Embed).await,
        EmbedSubaction::Recover => job_recover(service_context, JobKind::Embed).await,
    }
}

pub(super) async fn dispatch_ingest(
    service_context: &ServiceContext,
    req: IngestRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(IngestSubaction::Start) {
        IngestSubaction::Start => {
            let source = parse_ingest_source(&req, service_context.cfg.as_ref())?;
            let outcome = ingest_svc::ingest_start_with_context(
                service_context.cfg.as_ref(),
                source,
                service_context,
            )
            .await
            .map_err(internal_error)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        IngestSubaction::Status => job_status(service_context, JobKind::Ingest, req.job_id).await,
        IngestSubaction::Cancel => job_cancel(service_context, JobKind::Ingest, req.job_id).await,
        IngestSubaction::List => {
            job_list(service_context, JobKind::Ingest, req.limit, req.offset).await
        }
        IngestSubaction::Cleanup => job_cleanup(service_context, JobKind::Ingest).await,
        IngestSubaction::Clear => job_clear(service_context, JobKind::Ingest).await,
        IngestSubaction::Recover => job_recover(service_context, JobKind::Ingest).await,
    }
}

pub(super) async fn dispatch_scrape(
    service_context: &ServiceContext,
    req: ScrapeRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req
        .url
        .ok_or_else(|| ClientActionError::new("invalid_request", "url is required", false, None))?;
    let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode.map(map_render_mode),
        format: req.format.map(map_scrape_format),
        embed: req.embed,
        root_selector: req.root_selector,
        exclude_selector: req.exclude_selector,
        ..ConfigOverrides::default()
    });
    let result = scrape_svc::scrape(&cfg, &url, None)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({
        "url": result.url,
        "markdown": result.markdown,
        "output": result.output,
        "payload": result.payload,
        "artifact_handle": result.artifact_handle,
    }))
}

pub(super) async fn dispatch_screenshot(
    service_context: &ServiceContext,
    req: ScreenshotRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req
        .url
        .ok_or_else(|| ClientActionError::new("invalid_request", "url is required", false, None))?;
    let (width, height) = parse_viewport(
        req.viewport.as_deref(),
        service_context.cfg.viewport_width,
        service_context.cfg.viewport_height,
    )?;
    let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
        viewport_width: Some(width),
        viewport_height: Some(height),
        screenshot_full_page: req.full_page,
        ..ConfigOverrides::default()
    });
    let result = screenshot_svc::screenshot_capture(&cfg, &url)
        .await
        .map_err(internal_error)?;
    serde_json::to_value(result).map_err(|err| {
        ClientActionError::new(
            "internal",
            format!("serialize screenshot result: {err}"),
            false,
            None,
        )
    })
}

async fn job_status(
    service_context: &ServiceContext,
    kind: JobKind,
    raw_id: Option<String>,
) -> Result<serde_json::Value, ClientActionError> {
    let id = parse_job_id(raw_id.as_deref())?;
    let job = job_svc::job_status(service_context, kind, id)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "job": job }))
}

async fn job_cancel(
    service_context: &ServiceContext,
    kind: JobKind,
    raw_id: Option<String>,
) -> Result<serde_json::Value, ClientActionError> {
    let id = parse_job_id(raw_id.as_deref())?;
    let canceled = job_svc::cancel_job(service_context, kind, id)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }))
}

async fn job_list(
    service_context: &ServiceContext,
    kind: JobKind,
    limit: Option<i64>,
    offset: Option<usize>,
) -> Result<serde_json::Value, ClientActionError> {
    let limit = limit.unwrap_or(20).clamp(1, 500);
    let offset = offset.unwrap_or(0).min(i64::MAX as usize) as i64;
    let jobs = job_svc::list_jobs(service_context, kind, limit, offset)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "jobs": jobs, "limit": limit, "offset": offset }))
}

async fn job_cleanup(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let deleted = job_svc::cleanup_jobs(service_context, kind)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "deleted": deleted }))
}

async fn job_clear(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let deleted = job_svc::clear_jobs(service_context, kind)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "deleted": deleted }))
}

async fn job_recover(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let recovered = job_svc::recover_jobs(service_context, kind)
        .await
        .map_err(internal_error)?;
    Ok(serde_json::json!({ "recovered": recovered }))
}

fn parse_ingest_source(
    req: &IngestRequest,
    cfg: &Config,
) -> Result<ingest_svc::IngestSource, ClientActionError> {
    let source_type = req.source_type.clone().ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "source_type is required for ingest.start",
            false,
            None,
        )
    })?;
    match source_type {
        IngestSourceType::Github => {
            let repo = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target repo is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Github {
                repo,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Reddit => {
            let target = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Reddit { target })
        }
        IngestSourceType::Youtube => {
            let target = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Youtube { target })
        }
        IngestSourceType::Sessions => {
            let sessions =
                req.sessions
                    .clone()
                    .unwrap_or(crate::mcp::schema::SessionsIngestOptions {
                        claude: None,
                        codex: None,
                        gemini: None,
                        project: None,
                    });
            Ok(ingest_svc::IngestSource::Sessions {
                sessions_claude: sessions.claude.unwrap_or(false),
                sessions_codex: sessions.codex.unwrap_or(false),
                sessions_gemini: sessions.gemini.unwrap_or(false),
                sessions_project: sessions.project,
            })
        }
    }
}

fn apply_crawl_overrides(cfg: &Config, req: &CrawlRequest) -> Config {
    cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        render_mode: req.render_mode.map(map_render_mode),
        delay_ms: req.delay_ms,
        ..ConfigOverrides::default()
    })
}

fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

fn map_scrape_format(format: McpScrapeFormat) -> ScrapeFormat {
    match format {
        McpScrapeFormat::Markdown => ScrapeFormat::Markdown,
        McpScrapeFormat::Html => ScrapeFormat::Html,
        McpScrapeFormat::RawHtml => ScrapeFormat::RawHtml,
        McpScrapeFormat::Json => ScrapeFormat::Json,
    }
}

fn parse_viewport(
    raw: Option<&str>,
    fallback_width: u32,
    fallback_height: u32,
) -> Result<(u32, u32), ClientActionError> {
    let Some(raw) = raw else {
        return Ok((fallback_width, fallback_height));
    };
    let Some((width, height)) = raw.split_once('x') else {
        return Err(ClientActionError::new(
            "invalid_request",
            format!("invalid viewport '{raw}': expected WxH"),
            false,
            None,
        ));
    };
    let width = width.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport width '{width}': {err}"),
            false,
            None,
        )
    })?;
    let height = height.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport height '{height}': {err}"),
            false,
            None,
        )
    })?;
    if width == 0 || height == 0 {
        return Err(ClientActionError::new(
            "invalid_request",
            "viewport width and height must be greater than zero",
            false,
            None,
        ));
    }
    Ok((width, height))
}

fn parse_job_id(raw: Option<&str>) -> Result<Uuid, ClientActionError> {
    let raw = raw.ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "job_id is required",
            false,
            Some("include a UUID job_id for this lifecycle action".to_string()),
        )
    })?;
    Uuid::parse_str(raw).map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid job_id: {err}"),
            false,
            Some("job_id must be a UUID returned by a start action".to_string()),
        )
    })
}
