use crate::context::ServiceContext;
use crate::events::ServiceEvent;
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ArtifactHandle, CrawlJobResult, CrawlStartJob, CrawlStartResult, ExecutionMode,
    JobStartOutcome, StartDisposition,
};
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    JobStagePlan, MetadataMap, PipelinePhase,
};
use axon_core::config::Config;
use axon_core::http::validate_url;
use axon_crawl::engine::{SitemapDiscovery, discover_sitemap_urls};
use axon_jobs::backend::JobKind;
use axon_jobs::config_snapshot::config_snapshot_json;
use serde::{Deserialize, Serialize};
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

pub use axon_jobs::crawl::CrawlJob;

// ── Crawl bounds: the ONE place crawl page-cap policy lives ───────────────────
//
// Default page cap *and* hard ceiling, applied here in the services layer so
// every transport (CLI, MCP, HTTP) gets identical behavior. Transports are thin
// shims: they pass the raw requested value (`max_pages == 0` means "unspecified",
// the `Config` default) and never resolve defaults or caps themselves. This is
// why `axon crawl <url>` and the MCP `crawl` action now behave identically.
pub const DEFAULT_CRAWL_MAX_PAGES: u32 = 5_000;

/// Resolve the effective crawl page cap from a raw requested value.
///
/// - `0` (unspecified) → [`DEFAULT_CRAWL_MAX_PAGES`]
/// - greater than the cap → clamped to [`DEFAULT_CRAWL_MAX_PAGES`]
/// - otherwise the requested value unchanged
///
/// `allow_unbounded` is the operator escape hatch
/// (`AXON_ALLOW_UNBOUNDED_BROAD_CRAWL`): it passes the request through untouched,
/// including `0` for an intentional uncapped deep crawl.
pub fn resolve_crawl_max_pages(requested: u32, allow_unbounded: bool) -> u32 {
    if allow_unbounded {
        return requested;
    }
    match requested {
        0 => DEFAULT_CRAWL_MAX_PAGES,
        n if n > DEFAULT_CRAWL_MAX_PAGES => DEFAULT_CRAWL_MAX_PAGES,
        n => n,
    }
}

/// Return a crawl-effective copy of `cfg` with the page cap resolved. Both the
/// async and sync crawl entry points run config through this so the page-cap
/// default cannot diverge by transport.
pub fn apply_crawl_defaults(cfg: &Config) -> Config {
    let mut effective = cfg.clone();
    effective.max_pages = resolve_crawl_max_pages(cfg.max_pages, cfg.allow_unbounded_broad_crawl);
    effective
}

// --- Pure mapping helpers (no I/O, testable without live services) ---

fn predict_audit_report_path(output_dir: &Path, url: &str) -> PathBuf {
    let slug = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    output_dir
        .join("audit")
        .join(format!("{slug}-diff-report.json"))
}

pub use axon_crawl::predict_crawl_output_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SitemapDiscoveryStats {
    pub robots_declared_sitemaps: usize,
    pub seeded_default_sitemaps: usize,
    pub discovered_sitemap_documents: usize,
    pub parsed_sitemap_documents: usize,
    pub discovered_urls: usize,
    pub filtered_out_of_scope_host: usize,
    pub filtered_out_of_scope_path: usize,
    pub filtered_excluded_prefix: usize,
    pub failed_fetches: usize,
    pub parse_errors: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SitemapDiscoveryResult {
    pub urls: Vec<String>,
    pub stats: SitemapDiscoveryStats,
}

impl From<SitemapDiscovery> for SitemapDiscoveryResult {
    fn from(d: SitemapDiscovery) -> Self {
        Self {
            urls: d.urls,
            stats: SitemapDiscoveryStats {
                robots_declared_sitemaps: d.robots_declared_sitemaps,
                seeded_default_sitemaps: d.seeded_default_sitemaps,
                parsed_sitemap_documents: d.parsed_sitemap_documents,
                discovered_urls: d.discovered_urls,
                failed_fetches: d.failed_fetches,
                discovered_sitemap_documents: d.robots_declared_sitemaps
                    + d.seeded_default_sitemaps,
                filtered_out_of_scope_host: 0,
                filtered_out_of_scope_path: 0,
                filtered_excluded_prefix: 0,
                parse_errors: 0,
            },
        }
    }
}

pub async fn discover_sitemap_urls_with_robots(
    cfg: &Config,
    start_url: &str,
) -> Result<SitemapDiscoveryResult, Box<dyn Error>> {
    let discovery = discover_sitemap_urls(cfg, start_url).await?;
    Ok(SitemapDiscoveryResult::from(discovery))
}

pub fn predict_crawl_output_paths(output_dir: &Path, url: &str) -> Vec<String> {
    vec![
        output_dir.join("manifest.jsonl"),
        output_dir.join("markdown"),
        predict_audit_report_path(output_dir, url),
    ]
    .into_iter()
    .map(|path| path.to_string_lossy().into_owned())
    .collect()
}

fn crawl_artifact_kind(path: &Path) -> &'static str {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("manifest.jsonl") => "crawl-manifest",
        Some("markdown") => "crawl-markdown",
        Some(name) if name.ends_with("-diff-report.json") => "crawl-audit",
        _ => "crawl-output",
    }
}

fn crawl_artifact_handles(
    base_output_dir: &Path,
    paths: &[String],
    job_id: Option<&str>,
    url: Option<&str>,
) -> Vec<ArtifactHandle> {
    paths
        .iter()
        .filter_map(|path| {
            let path_buf = PathBuf::from(path);
            ArtifactHandle::try_from_path(
                crawl_artifact_kind(&path_buf),
                base_output_dir,
                &path_buf,
                0,
                None,
                job_id.map(ToString::to_string),
                url.map(ToString::to_string),
            )
        })
        .collect()
}

pub fn map_crawl_start_result(
    base_output_dir: &Path,
    jobs: &[(String, String)],
) -> CrawlStartResult {
    let jobs: Vec<CrawlStartJob> = jobs
        .iter()
        .map(|(url, job_id)| {
            let output_dir = predict_crawl_output_dir(base_output_dir, url, job_id);
            let predicted_paths = predict_crawl_output_paths(&output_dir, url);
            CrawlStartJob {
                job_id: job_id.clone(),
                url: url.clone(),
                output_dir: output_dir.to_string_lossy().into_owned(),
                predicted_artifact_handles: crawl_artifact_handles(
                    base_output_dir,
                    &predicted_paths,
                    Some(job_id),
                    Some(url),
                ),
                predicted_paths,
            }
        })
        .collect();
    let job_ids = jobs.iter().map(|job| job.job_id.clone()).collect();
    let output_dir = jobs.first().map(|job| job.output_dir.clone());
    let predicted_paths = jobs
        .first()
        .map(|job| job.predicted_paths.clone())
        .unwrap_or_default();
    let predicted_artifact_handles = jobs
        .first()
        .map(|job| job.predicted_artifact_handles.clone())
        .unwrap_or_default();
    CrawlStartResult {
        job_ids,
        output_dir,
        predicted_paths,
        predicted_artifact_handles,
        jobs,
    }
}

pub fn map_crawl_job_result(payload: serde_json::Value) -> CrawlJobResult {
    map_crawl_job_result_with_root(payload, None)
}

pub fn map_crawl_job_result_with_root(
    payload: serde_json::Value,
    base_output_dir: Option<&Path>,
) -> CrawlJobResult {
    let output_files = output_files_from_payload(&payload);
    let job_id = payload
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| payload.get("job_id").and_then(|value| value.as_str()));
    let url = payload
        .get("url")
        .and_then(|value| value.as_str())
        .or_else(|| payload.get("target").and_then(|value| value.as_str()));
    let output_file_handles = base_output_dir
        .zip(output_files.as_ref())
        .map(|(root, files)| crawl_artifact_handles(root, files, job_id, url))
        .unwrap_or_default();
    CrawlJobResult {
        payload,
        output_files,
        output_file_handles,
    }
}

fn output_files_from_payload(payload: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(files) = payload.get("output_files").and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
    }) {
        return Some(files);
    }
    let result = payload.get("result").and_then(serde_json::Value::as_object);
    let mut files = Vec::new();
    if let Some(output_dir) = result
        .and_then(|value| value.get("output_dir"))
        .and_then(serde_json::Value::as_str)
    {
        files.push(PathBuf::from(output_dir).join("manifest.jsonl"));
        files.push(PathBuf::from(output_dir).join("markdown"));
    }
    if let Some(output_path) = result
        .and_then(|value| value.get("output_path"))
        .and_then(serde_json::Value::as_str)
    {
        let output_path = PathBuf::from(output_path);
        if !files.iter().any(|path| path == &output_path) {
            files.push(output_path);
        }
    }
    if files.is_empty() {
        None
    } else {
        Some(
            files
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        )
    }
}

// --- Service functions ---

pub async fn crawl_start_with_context(
    cfg: &Config,
    urls: &[String],
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    caller: Option<&AuthSnapshot>,
) -> Result<JobStartOutcome<CrawlStartResult>, Box<dyn Error>> {
    // tx is accepted for API compatibility but not used in the SQLite-only path
    let _ = tx;
    if urls.is_empty() {
        return Err("No URLs provided for crawl".into());
    }

    // Resolve crawl page-cap policy HERE (services layer) so the enqueued job
    // snapshot carries the effective cap regardless of which transport called us.
    let effective = apply_crawl_defaults(cfg);
    let config_json = config_snapshot_json(&effective)?;

    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;

    // One unified job per URL, matching the legacy per-URL job granularity.
    let mut jobs = Vec::with_capacity(urls.len());
    for url in urls {
        validate_url(url).map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
        let descriptor = store
            .create(JobCreateRequest {
                request_id: None,
                job_kind: UnifiedJobKind::Crawl,
                job_intent: JobIntent::Index,
                source_id: None,
                watch_id: None,
                parent_job_id: None,
                root_job_id: None,
                attempt: 1,
                priority: JobPriority::Normal,
                idempotency_key: None,
                stage_plan: vec![JobStagePlan {
                    phase: PipelinePhase::Fetching,
                    required: true,
                    provider_requirements: Vec::new(),
                    estimated_items: None,
                }],
                request: Some(serde_json::json!({
                    "urls": [url],
                    "config_json": config_json,
                })),
                auth_snapshot: caller
                    .cloned()
                    .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
                config_snapshot_id: None,
                requirements: MetadataMap::new(),
                result_schema: Some("crawl_result".to_string()),
                warnings: Vec::new(),
                error: None,
                metadata: MetadataMap::new(),
                deadline_at: None,
            })
            .await
            .map_err(|e| -> Box<dyn Error> { e.message.into() })?;
        jobs.push((url.clone(), descriptor.job_id.0.to_string()));
    }
    service_context.notify_unified();

    let result = map_crawl_start_result(&cfg.output_dir, &jobs);
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result,
    })
}

/// Look up the current state of a crawl job by its UUID.
pub async fn crawl_status(
    service_context: &ServiceContext,
    job_id: Uuid,
) -> Result<Option<CrawlJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Crawl, job_id).await?;
    let Some(job) = job else { return Ok(None) };
    let payload = serde_json::to_value(job)?;
    Ok(Some(map_crawl_job_result_with_root(
        payload,
        Some(&service_context.cfg.output_dir),
    )))
}

pub async fn crawl_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<CrawlJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Crawl, limit, offset).await?;
    Ok(map_crawl_job_result_with_root(
        serde_json::to_value(jobs)?,
        Some(&service_context.cfg.output_dir),
    ))
}

pub async fn crawl_cancel(
    service_context: &ServiceContext,
    job_id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Crawl, job_id).await
}

pub async fn crawl_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Crawl).await
}

pub async fn crawl_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Crawl).await
}

pub async fn crawl_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Crawl).await
}

pub async fn crawl_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Crawl).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

#[cfg(test)]
#[path = "crawl_tests.rs"]
mod tests;
