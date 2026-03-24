use crate::crates::core::config::Config;
use crate::crates::core::content::url_to_domain;
use crate::crates::jobs::crawl;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    CrawlJobResult, CrawlStartJob, CrawlStartResult, JobListResult,
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
#[must_use = "crawl_start returns a Result that should be handled"]
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

/// Look up the current state of a crawl job by its UUID.
pub async fn crawl_status(cfg: &Config, job_id: Uuid) -> Result<CrawlJobResult, Box<dyn Error>> {
    let job = crawl::get_job(cfg, job_id).await?;
    let payload = serde_json::to_value(job)?;
    Ok(map_crawl_job_result(payload))
}

pub async fn crawl_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<CrawlJobResult, Box<dyn Error>> {
    let jobs = crawl::list_jobs(cfg, limit, offset).await?;
    Ok(map_crawl_job_result(serde_json::to_value(jobs)?))
}

pub async fn crawl_cancel(cfg: &Config, job_id: Uuid) -> Result<bool, Box<dyn Error>> {
    crawl::cancel_job(cfg, job_id).await
}

pub async fn crawl_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    crawl::cleanup_jobs(cfg).await
}

pub async fn crawl_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    crawl::clear_jobs(cfg).await
}

pub async fn crawl_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    crawl::recover_stale_crawl_jobs(cfg).await
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
) -> Result<JobListResult<CrawlJob>, Box<dyn Error>> {
    let (jobs, total) = tokio::join!(crawl::list_jobs(cfg, limit, offset), crawl::count_jobs(cfg),);
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}

pub async fn crawl_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    crawl::run_worker(cfg).await
}

#[cfg(test)]
mod tests {
    use super::{map_crawl_start_result, predict_crawl_output_dir};
    use std::path::Path;

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
