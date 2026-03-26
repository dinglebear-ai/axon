use crate::crates::core::config::Config;
use crate::crates::core::content::url_to_domain;
use crate::crates::jobs::backend::JobKind;
use crate::crates::jobs::crawl;
use crate::crates::jobs::status::JobStatus;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::jobs::WorkerMode;
use crate::crates::services::types::{
    CrawlJobResult, CrawlStartJob, CrawlStartResult, ExecutionMode, JobStartOutcome,
    StartDisposition,
};
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

pub use crate::crates::jobs::crawl::CrawlJob;

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

pub fn map_crawl_start_result(
    base_output_dir: &Path,
    jobs: &[(String, String)],
) -> CrawlStartResult {
    let jobs: Vec<CrawlStartJob> = jobs
        .iter()
        .map(|(url, job_id)| {
            let output_dir = predict_crawl_output_dir(base_output_dir, url, job_id);
            CrawlStartJob {
                job_id: job_id.clone(),
                url: url.clone(),
                output_dir: output_dir.to_string_lossy().into_owned(),
                predicted_paths: predict_crawl_output_paths(&output_dir, url),
            }
        })
        .collect();
    let job_ids = jobs.iter().map(|job| job.job_id.clone()).collect();
    let output_dir = jobs.first().map(|job| job.output_dir.clone());
    let predicted_paths = jobs
        .first()
        .map(|job| job.predicted_paths.clone())
        .unwrap_or_default();
    CrawlStartResult {
        job_ids,
        output_dir,
        predicted_paths,
        jobs,
    }
}

pub fn map_crawl_job_result(payload: serde_json::Value) -> CrawlJobResult {
    let output_files = payload.get("output_files").and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
    });
    CrawlJobResult {
        payload,
        output_files,
    }
}

// --- Service functions ---

/// Enqueue one or more crawl jobs and return their job IDs immediately.
/// Fire-and-forget: jobs are inserted into the queue and this function returns
/// without waiting for the crawl to complete.
pub async fn crawl_start(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<CrawlStartResult, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("crawl_start: no URLs provided".into());
    }

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing crawl jobs for {} URL(s)", urls.len()),
        },
    )
    .await;

    let url_refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let jobs = crawl::start_crawl_jobs_batch(cfg, &url_refs).await?;

    let jobs: Vec<(String, String)> = jobs
        .into_iter()
        .map(|(url, id)| (url, id.to_string()))
        .collect();

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueued {} crawl job(s)", jobs.len()),
        },
    )
    .await;

    Ok(map_crawl_start_result(&cfg.output_dir, &jobs))
}

pub async fn crawl_start_with_context(
    cfg: &Config,
    urls: &[String],
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<CrawlStartResult>, Box<dyn Error>> {
    if !cfg.lite_mode {
        let result = crawl_start(cfg, urls, tx).await?;
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::Enqueued,
            result,
        });
    }

    let backend = service_context
        .require_job_backend()
        .map_err(|e| -> Box<dyn Error> { e })?;
    let mut jobs = Vec::with_capacity(urls.len());
    for url in urls {
        let job_id = backend
            .enqueue(crate::crates::jobs::backend::JobPayload::Crawl {
                url: url.clone(),
                config_json: "{}".to_string(),
            })
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        jobs.push((url.clone(), job_id.to_string()));
    }

    let result = map_crawl_start_result(&cfg.output_dir, &jobs);
    for job in &result.jobs {
        let job_id = Uuid::parse_str(&job.job_id)?;
        let final_status = backend
            .wait_for_job(job_id, JobKind::Crawl)
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        if final_status == "failed" {
            if let Ok(Some(err)) = backend.job_errors(job_id, JobKind::Crawl).await {
                return Err(format!("crawl job {job_id} failed: {err}").into());
            }
            return Err(format!("crawl job {job_id} failed").into());
        }
        wait_for_pending_embed_jobs(backend).await;
    }

    Ok(JobStartOutcome {
        disposition: StartDisposition::Completed,
        execution_mode: ExecutionMode::InProcess,
        result,
    })
}

/// Look up the current state of a crawl job by its UUID.
pub async fn crawl_status(cfg: &Config, job_id: Uuid) -> Result<CrawlJobResult, Box<dyn Error>> {
    let job = job_service::job_status(cfg, JobKind::Crawl, job_id).await?;
    let payload = serde_json::to_value(job)?;
    Ok(map_crawl_job_result(payload))
}

pub async fn crawl_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<CrawlJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(cfg, JobKind::Crawl, limit, offset).await?;
    Ok(map_crawl_job_result(serde_json::to_value(jobs)?))
}

pub async fn crawl_cancel(cfg: &Config, job_id: Uuid) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(cfg, JobKind::Crawl, job_id).await
}

pub async fn crawl_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(cfg, JobKind::Crawl).await
}

pub async fn crawl_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(cfg, JobKind::Crawl).await
}

pub async fn crawl_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(cfg, JobKind::Crawl).await
}

pub async fn crawl_status_raw(
    cfg: &Config,
    job_id: Uuid,
) -> Result<Option<CrawlJob>, Box<dyn Error>> {
    crawl::get_job(cfg, job_id).await
}

pub async fn crawl_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<CrawlJob>, Box<dyn Error>> {
    crawl::list_jobs(cfg, limit, offset).await
}

pub async fn crawl_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match job_service::run_worker(cfg, JobKind::Crawl).await? {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

async fn wait_for_pending_embed_jobs(
    backend: &std::sync::Arc<dyn crate::crates::jobs::backend::JobBackend>,
) {
    loop {
        match backend.list_jobs(JobKind::Embed).await {
            Ok(jobs) => {
                let active = jobs.iter().any(|job| {
                    job.status == JobStatus::Pending || job.status == JobStatus::Running
                });
                if !active {
                    break;
                }
            }
            Err(_) => break,
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{crawl_start_with_context, map_crawl_start_result, predict_crawl_output_dir};
    use crate::crates::jobs::backend::{
        BackendResult, JobBackend, JobId, JobPayload, JobStatusRow, JobSummary,
    };
    use crate::crates::jobs::common::test_config;
    use crate::crates::jobs::status::JobStatus;
    use crate::crates::services::context::ServiceContext;
    use crate::crates::services::types::{ExecutionMode, StartDisposition};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::path::Path;
    use std::sync::Arc;
    use uuid::Uuid;

    struct CompletedLiteBackend;

    #[async_trait]
    impl JobBackend for CompletedLiteBackend {
        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<JobId> {
            Ok(Uuid::new_v4())
        }

        async fn job_status(
            &self,
            id: JobId,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<Option<JobStatusRow>> {
            Ok(Some(JobStatusRow {
                id,
                status: JobStatus::Completed,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                started_at: None,
                finished_at: Some(Utc::now()),
                error_text: None,
                result_json: None,
            }))
        }

        async fn cancel_job(
            &self,
            _id: JobId,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<bool> {
            Ok(false)
        }

        async fn list_jobs(
            &self,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<Vec<JobSummary>> {
            Ok(vec![])
        }

        async fn cleanup_jobs(
            &self,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<u64> {
            Ok(0)
        }

        async fn clear_jobs(
            &self,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<u64> {
            Ok(0)
        }

        async fn job_errors(
            &self,
            _id: JobId,
            _kind: crate::crates::jobs::backend::JobKind,
        ) -> BackendResult<Option<String>> {
            Ok(None)
        }
    }

    #[test]
    fn map_crawl_start_result_includes_predicted_output_paths() {
        let result = map_crawl_start_result(
            Path::new("/tmp/axon-output"),
            &[("https://docs.rs".to_string(), "job-123".to_string())],
        );

        assert_eq!(result.job_ids, vec!["job-123".to_string()]);
        assert_eq!(
            result.output_dir,
            Some("/tmp/axon-output/domains/docs.rs/job-123".to_string())
        );
        assert_eq!(
            result.predicted_paths,
            vec![
                "/tmp/axon-output/domains/docs.rs/job-123/manifest.jsonl".to_string(),
                "/tmp/axon-output/domains/docs.rs/job-123/markdown".to_string(),
                "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json"
                    .to_string(),
            ]
        );
        assert_eq!(result.jobs.len(), 1);
        let job = &result.jobs[0];
        assert_eq!(job.url, "https://docs.rs");
        assert_eq!(
            job.output_dir,
            "/tmp/axon-output/domains/docs.rs/job-123".to_string()
        );
        assert_eq!(
            job.predicted_paths,
            vec![
                "/tmp/axon-output/domains/docs.rs/job-123/manifest.jsonl".to_string(),
                "/tmp/axon-output/domains/docs.rs/job-123/markdown".to_string(),
                "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn predict_crawl_output_dir_uses_runtime_job_layout() {
        let output_dir = predict_crawl_output_dir(
            Path::new(".cache/axon-rust/output"),
            "https://[::1]:8080/docs",
            "job-456",
        );

        assert_eq!(
            output_dir,
            Path::new(".cache/axon-rust/output")
                .join("domains")
                .join("___1_")
                .join("job-456")
        );
    }

    #[tokio::test]
    async fn crawl_start_with_context_completes_in_lite_mode() {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await
            .expect("service context")
            .with_job_backend(Arc::new(CompletedLiteBackend));

        let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .expect("lite crawl outcome");

        assert_eq!(outcome.disposition, StartDisposition::Completed);
        assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
        assert_eq!(outcome.result.jobs.len(), 1);
        assert_eq!(outcome.result.jobs[0].url, cfg.start_url);
    }

    #[test]
    fn map_crawl_job_result_preserves_output_files() {
        let result = super::map_crawl_job_result(serde_json::json!({
            "phase": "completed",
            "output_files": [
                "/tmp/axon-output/manifest.jsonl",
                "/tmp/axon-output/markdown/index.md"
            ]
        }));

        let output_files = result
            .output_files
            .as_ref()
            .expect("output_files should be mapped from payload");
        assert_eq!(
            output_files,
            &vec![
                "/tmp/axon-output/manifest.jsonl".to_string(),
                "/tmp/axon-output/markdown/index.md".to_string(),
            ]
        );
    }
}
