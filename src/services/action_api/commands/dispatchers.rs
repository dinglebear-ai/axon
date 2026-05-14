use crate::core::config::{Config, ConfigOverrides, RenderMode, ScrapeFormat};
use crate::core::content::url_to_filename;
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
use crate::services::scrape as scrape_svc;
use crate::services::screenshot as screenshot_svc;
use crate::services::types::ClientActionError;
use uuid::Uuid;

use super::helpers::{
    apply_crawl_overrides, internal_message, map_render_mode, map_scrape_format,
    parse_ingest_source, parse_viewport,
};
use super::job_ops::{job_cancel, job_cleanup, job_clear, job_list, job_recover, job_status};

pub async fn dispatch_crawl(
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
                .map_err(internal_message)?;
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
            let id = super::parse_job_id(req.job_id.as_deref())?;
            let result = crawl_svc::crawl_status(service_context, id)
                .await
                .map_err(internal_message)?;
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
                .map_err(internal_message)?;
            Ok(serde_json::json!({
                "jobs": result.payload,
                "limit": limit,
                "offset": offset,
            }))
        }
        CrawlSubaction::Cancel => {
            let id = super::parse_job_id(req.job_id.as_deref())?;
            let canceled = crawl_svc::crawl_cancel(service_context, id)
                .await
                .map_err(internal_message)?;
            Ok(serde_json::json!({
                "job_id": id.to_string(),
                "canceled": canceled,
            }))
        }
        CrawlSubaction::Cleanup => {
            let deleted = crawl_svc::crawl_cleanup(service_context)
                .await
                .map_err(internal_message)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Clear => {
            let deleted = crawl_svc::crawl_clear(service_context)
                .await
                .map_err(internal_message)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Recover => {
            let recovered = crawl_svc::crawl_recover(service_context)
                .await
                .map_err(internal_message)?;
            Ok(serde_json::json!({ "recovered": recovered }))
        }
    }
}

pub async fn dispatch_extract(
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
                    .map_err(internal_message)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        ExtractSubaction::Status => {
            job_status(
                service_context,
                crate::jobs::backend::JobKind::Extract,
                req.job_id,
            )
            .await
        }
        ExtractSubaction::Cancel => {
            job_cancel(
                service_context,
                crate::jobs::backend::JobKind::Extract,
                req.job_id,
            )
            .await
        }
        ExtractSubaction::List => {
            job_list(
                service_context,
                crate::jobs::backend::JobKind::Extract,
                req.limit,
                req.offset,
            )
            .await
        }
        ExtractSubaction::Cleanup => {
            job_cleanup(service_context, crate::jobs::backend::JobKind::Extract).await
        }
        ExtractSubaction::Clear => {
            job_clear(service_context, crate::jobs::backend::JobKind::Extract).await
        }
        ExtractSubaction::Recover => {
            job_recover(service_context, crate::jobs::backend::JobKind::Extract).await
        }
    }
}

pub async fn dispatch_embed(
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
            .map_err(internal_message)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        EmbedSubaction::Status => {
            job_status(
                service_context,
                crate::jobs::backend::JobKind::Embed,
                req.job_id,
            )
            .await
        }
        EmbedSubaction::Cancel => {
            job_cancel(
                service_context,
                crate::jobs::backend::JobKind::Embed,
                req.job_id,
            )
            .await
        }
        EmbedSubaction::List => {
            job_list(
                service_context,
                crate::jobs::backend::JobKind::Embed,
                req.limit,
                req.offset,
            )
            .await
        }
        EmbedSubaction::Cleanup => {
            job_cleanup(service_context, crate::jobs::backend::JobKind::Embed).await
        }
        EmbedSubaction::Clear => {
            job_clear(service_context, crate::jobs::backend::JobKind::Embed).await
        }
        EmbedSubaction::Recover => {
            job_recover(service_context, crate::jobs::backend::JobKind::Embed).await
        }
    }
}

pub async fn dispatch_ingest(
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
            .map_err(internal_message)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
        IngestSubaction::Status => {
            job_status(
                service_context,
                crate::jobs::backend::JobKind::Ingest,
                req.job_id,
            )
            .await
        }
        IngestSubaction::Cancel => {
            job_cancel(
                service_context,
                crate::jobs::backend::JobKind::Ingest,
                req.job_id,
            )
            .await
        }
        IngestSubaction::List => {
            job_list(
                service_context,
                crate::jobs::backend::JobKind::Ingest,
                req.limit,
                req.offset,
            )
            .await
        }
        IngestSubaction::Cleanup => {
            job_cleanup(service_context, crate::jobs::backend::JobKind::Ingest).await
        }
        IngestSubaction::Clear => {
            job_clear(service_context, crate::jobs::backend::JobKind::Ingest).await
        }
        IngestSubaction::Recover => {
            job_recover(service_context, crate::jobs::backend::JobKind::Ingest).await
        }
    }
}

pub async fn dispatch_scrape(
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
        output_path: Some(Some(server_scrape_output_path(
            service_context.cfg.as_ref(),
            &url,
        ))),
        ..ConfigOverrides::default()
    });
    let result = scrape_svc::scrape(&cfg, &url, None)
        .await
        .map_err(internal_message)?;
    if let Some(path) = cfg.output_path.as_ref() {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| internal_message(format!("create scrape output dir: {err}")))?;
        }
        tokio::fs::write(path, &result.output)
            .await
            .map_err(|err| internal_message(format!("write scrape output: {err}")))?;
    }
    Ok(serde_json::json!({
        "url": result.url,
        "markdown": result.markdown,
        "output": result.output,
        "payload": result.payload,
        "artifact_handle": result.artifact_handle,
    }))
}

fn server_scrape_output_path(cfg: &Config, url: &str) -> std::path::PathBuf {
    cfg.output_dir
        .join("scrape-markdown")
        .join("runs")
        .join(Uuid::new_v4().to_string())
        .join(url_to_filename(url, 1))
}

pub async fn dispatch_screenshot(
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
        .map_err(internal_message)?;
    serde_json::to_value(result).map_err(|err| {
        ClientActionError::new(
            "internal",
            format!("serialize screenshot result: {err}"),
            false,
            None,
        )
    })
}
