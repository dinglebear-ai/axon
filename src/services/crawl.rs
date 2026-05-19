use crate::core::config::Config;
use crate::core::content::url_to_domain;
use crate::jobs::backend::JobKind;
use crate::jobs::config_snapshot::config_snapshot_json;
use crate::services::context::ServiceContext;
use crate::services::events::ServiceEvent;
use crate::services::jobs as job_service;
use crate::services::runtime::WorkerMode;
use crate::services::types::{
    ArtifactHandle, CrawlJobResult, CrawlStartJob, CrawlStartResult, ExecutionMode,
    JobStartOutcome, StartDisposition,
};
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

pub use crate::jobs::crawl::CrawlJob;

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

pub fn predict_crawl_output_dir(base_output_dir: &Path, url: &str, job_id: &str) -> PathBuf {
    base_output_dir
        .join("domains")
        .join(url_to_domain(url))
        .join(job_id)
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
) -> Result<JobStartOutcome<CrawlStartResult>, Box<dyn Error>> {
    // tx is accepted for API compatibility but not used in the SQLite-only path
    let _ = tx;
    if urls.is_empty() {
        return Err("No URLs provided for crawl".into());
    }

    let mut jobs = Vec::with_capacity(urls.len());
    for url in urls {
        let job_id = service_context
            .jobs
            .enqueue(crate::jobs::backend::JobPayload::Crawl {
                url: url.clone(),
                config_json: config_snapshot_json(cfg)?,
            })
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        jobs.push((url.clone(), job_id.to_string()));
    }

    let result = map_crawl_start_result(&cfg.output_dir, &jobs);

    if !cfg.wait {
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::InProcess,
            result,
        });
    }

    for job in &result.jobs {
        let job_id = Uuid::parse_str(&job.job_id)?;
        let final_status = service_context
            .jobs
            .wait_for_job(job_id, JobKind::Crawl)
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        match final_status.as_str() {
            "failed" => {
                if let Ok(Some(err)) = service_context
                    .jobs
                    .job_errors(job_id, JobKind::Crawl)
                    .await
                {
                    return Err(format!("crawl job {job_id} failed: {err}").into());
                }
                return Err(format!("crawl job {job_id} failed").into());
            }
            "canceled" => {
                if let Ok(Some(err)) = service_context
                    .jobs
                    .job_errors(job_id, JobKind::Crawl)
                    .await
                {
                    return Err(format!("crawl job {job_id} canceled: {err}").into());
                }
                return Err(format!("crawl job {job_id} canceled").into());
            }
            _ => {}
        }
        wait_for_crawl_embed_dependency(service_context, job_id).await?;
    }

    Ok(JobStartOutcome {
        disposition: StartDisposition::Completed,
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

async fn wait_for_crawl_embed_dependency(
    service_context: &ServiceContext,
    crawl_job_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let Some(job) = service_context
        .jobs
        .job_status(JobKind::Crawl, crawl_job_id)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?
    else {
        return Ok(());
    };

    let Some(embed_job_id) = job
        .result_json
        .as_ref()
        .and_then(|result| result.get("embed_job_id"))
        .and_then(|value| value.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return Ok(());
    };

    let final_status = service_context
        .jobs
        .wait_for_job(embed_job_id, JobKind::Embed)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;

    match final_status.as_str() {
        "failed" => {
            if let Ok(Some(err)) = service_context
                .jobs
                .job_errors(embed_job_id, JobKind::Embed)
                .await
            {
                return Err(format!("crawl embed job {embed_job_id} failed: {err}").into());
            }
            Err(format!("crawl embed job {embed_job_id} failed").into())
        }
        "canceled" => {
            if let Ok(Some(err)) = service_context
                .jobs
                .job_errors(embed_job_id, JobKind::Embed)
                .await
            {
                return Err(format!("crawl embed job {embed_job_id} canceled: {err}").into());
            }
            Err(format!("crawl embed job {embed_job_id} canceled").into())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "crawl_tests.rs"]
mod tests;
