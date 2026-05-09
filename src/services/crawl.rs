use crate::core::config::Config;
use crate::core::content::url_to_domain;
use crate::jobs::backend::JobKind;
use crate::jobs::lite::config_snapshot::lite_config_snapshot_json;
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
    // tx is accepted for API compatibility but not used in the lite-only path
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
                config_json: lite_config_snapshot_json(cfg)?,
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
) -> Result<CrawlJobResult, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Crawl, job_id).await?;
    let payload = serde_json::to_value(job)?;
    Ok(map_crawl_job_result_with_root(
        payload,
        Some(&service_context.cfg.output_dir),
    ))
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
mod tests {
    use super::{crawl_start_with_context, map_crawl_start_result, predict_crawl_output_dir};
    use crate::core::config::Config;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::services::context::ServiceContext;
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use crate::services::types::{ExecutionMode, StartDisposition};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn test_config(start_url: &str) -> Config {
        let mut cfg = Config::default_lite();
        cfg.start_url = start_url.to_string();
        cfg
    }

    struct CompletedLiteRuntime;
    struct CanceledLiteRuntime;
    struct CrawlWithDependentEmbedRuntime {
        crawl_job_id: Uuid,
        embed_job_id: Uuid,
        wait_calls: Mutex<Vec<(Uuid, JobKind)>>,
    }

    /// Generate a complete `#[async_trait] impl ServiceJobRuntime for $t` with all
    /// no-op shared methods plus caller-supplied `wait_for_job` and `job_errors` bodies.
    ///
    /// The macro wraps the entire `impl` block so that `#[async_trait]` processes the
    /// expanded token stream — inner macro invocations are not visible to attribute
    /// proc macros before expansion.
    macro_rules! impl_noop_runtime_for {
        ($t:ty, $wait_result:expr, $errors_result:expr) => {
            #[async_trait]
            impl ServiceJobRuntime for $t {
                fn mode_name(&self) -> &'static str {
                    "test"
                }

                async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
                    Ok(Uuid::new_v4())
                }

                async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
                    Ok(false)
                }

                async fn list_jobs(
                    &self,
                    _kind: JobKind,
                    _limit: i64,
                    _offset: i64,
                ) -> Result<
                    Vec<crate::services::types::ServiceJob>,
                    Box<dyn std::error::Error + Send + Sync>,
                > {
                    Ok(vec![])
                }

                async fn job_status(
                    &self,
                    _kind: JobKind,
                    _id: Uuid,
                ) -> Result<
                    Option<crate::services::types::ServiceJob>,
                    Box<dyn std::error::Error + Send + Sync>,
                > {
                    Ok(None)
                }

                async fn cancel_job(
                    &self,
                    _kind: JobKind,
                    _id: Uuid,
                ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(false)
                }

                async fn cleanup_jobs(
                    &self,
                    _kind: JobKind,
                ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(0)
                }

                async fn clear_jobs(
                    &self,
                    _kind: JobKind,
                ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(0)
                }

                async fn recover_jobs(
                    &self,
                    _kind: JobKind,
                    _stale_threshold_ms: i64,
                ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(0)
                }

                async fn count_jobs(
                    &self,
                    _kind: JobKind,
                ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(0)
                }

                async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
                    $wait_result
                }

                async fn job_errors(
                    &self,
                    _id: Uuid,
                    _kind: JobKind,
                ) -> BackendResult<Option<String>> {
                    $errors_result
                }
            }
        };
    }

    impl_noop_runtime_for!(CompletedLiteRuntime, Ok("completed".to_string()), Ok(None));
    impl_noop_runtime_for!(
        CanceledLiteRuntime,
        Ok("canceled".to_string()),
        Ok(Some("user canceled".to_string()))
    );

    #[async_trait]
    impl ServiceJobRuntime for CrawlWithDependentEmbedRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
            Ok(self.crawl_job_id)
        }

        async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String> {
            self.wait_calls.lock().unwrap().push((id, kind));
            Ok("completed".to_string())
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            panic!("crawl completion must not globally drain unrelated active jobs")
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }

        async fn job_status(
            &self,
            kind: JobKind,
            id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn std::error::Error + Send + Sync>> {
            if kind != JobKind::Crawl || id != self.crawl_job_id {
                return Ok(None);
            }
            let now = Utc::now();
            Ok(Some(ServiceJob {
                id,
                status: "completed".to_string(),
                created_at: now,
                updated_at: now,
                started_at: Some(now),
                finished_at: Some(now),
                error_text: None,
                url: Some("https://docs.rs".to_string()),
                source_type: None,
                target: None,
                urls_json: None,
                result_json: Some(serde_json::json!({
                    "embed_job_id": self.embed_job_id.to_string()
                })),
                config_json: None,
            }))
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(
            &self,
            _kind: JobKind,
        ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(
            &self,
            _kind: JobKind,
        ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
            Ok(0)
        }

        async fn count_jobs(
            &self,
            _kind: JobKind,
        ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
            Ok(0)
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
        assert_eq!(result.predicted_artifact_handles.len(), 3);
        assert_eq!(
            result.predicted_artifact_handles[0].relative_path,
            "domains/docs.rs/job-123/manifest.jsonl"
        );
        assert_eq!(
            result.predicted_artifact_handles[0].job_id.as_deref(),
            Some("job-123")
        );
        assert_eq!(
            result.predicted_artifact_handles[0].url.as_deref(),
            Some("https://docs.rs")
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
        assert_eq!(job.predicted_artifact_handles.len(), 3);
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
    async fn crawl_start_with_context_completes_in_lite_mode()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        cfg.wait = true;
        let ctx =
            ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

        let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .map_err(|e| e.to_string())?;

        assert_eq!(outcome.disposition, StartDisposition::Completed);
        assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
        assert_eq!(outcome.result.jobs.len(), 1);
        assert_eq!(outcome.result.jobs[0].url, cfg.start_url);
        Ok(())
    }

    #[tokio::test]
    async fn crawl_start_waits_for_only_its_dependent_embed_job()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        cfg.wait = true;
        let crawl_job_id = Uuid::new_v4();
        let embed_job_id = Uuid::new_v4();
        let runtime = Arc::new(CrawlWithDependentEmbedRuntime {
            crawl_job_id,
            embed_job_id,
            wait_calls: Mutex::new(Vec::new()),
        });
        let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

        let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .map_err(|e| e.to_string())?;

        assert_eq!(outcome.disposition, StartDisposition::Completed);
        assert_eq!(
            *runtime.wait_calls.lock().unwrap(),
            vec![
                (crawl_job_id, JobKind::Crawl),
                (embed_job_id, JobKind::Embed)
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn crawl_start_with_context_rejects_empty_urls_in_lite_mode() {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        let ctx =
            ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

        let err = crawl_start_with_context(&cfg, &[], &ctx, None)
            .await
            .expect_err("empty urls must fail");
        assert!(err.to_string().contains("No URLs provided"));
    }

    #[tokio::test]
    async fn crawl_start_with_context_enqueues_without_blocking_in_lite_mode()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        cfg.wait = false;
        let ctx =
            ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

        let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .map_err(|e| e.to_string())?;

        assert_eq!(outcome.disposition, StartDisposition::Enqueued);
        assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
        assert_eq!(outcome.result.jobs.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn crawl_start_with_context_surfaces_canceled_jobs_in_lite_mode() {
        let mut cfg = test_config("https://docs.rs");
        cfg.lite_mode = true;
        cfg.wait = true;
        let ctx =
            ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CanceledLiteRuntime));

        let err = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .expect_err("canceled crawl must fail");
        assert!(err.to_string().contains("canceled"));
    }

    #[test]
    fn map_crawl_job_result_preserves_output_files() {
        let result = super::map_crawl_job_result_with_root(
            serde_json::json!({
                "id": "job-123",
                "url": "https://docs.rs",
                "phase": "completed",
                "output_files": [
                    "/tmp/axon-output/manifest.jsonl",
                    "/tmp/axon-output/markdown/index.md"
                ]
            }),
            Some(Path::new("/tmp/axon-output")),
        );

        assert_eq!(
            result.output_files.as_ref(),
            Some(&vec![
                "/tmp/axon-output/manifest.jsonl".to_string(),
                "/tmp/axon-output/markdown/index.md".to_string(),
            ])
        );
        assert_eq!(result.output_file_handles.len(), 2);
        assert_eq!(
            result.output_file_handles[0].relative_path,
            "manifest.jsonl"
        );
        assert_eq!(
            result.output_file_handles[0].job_id.as_deref(),
            Some("job-123")
        );
        assert_eq!(
            result.output_file_handles[0].url.as_deref(),
            Some("https://docs.rs")
        );
    }

    #[test]
    fn map_crawl_job_result_derives_handles_from_result_json_paths() {
        let result = super::map_crawl_job_result_with_root(
            serde_json::json!({
                "id": "job-456",
                "url": "https://docs.rs",
                "result": {
                    "output_dir": "/tmp/axon-output/domains/docs.rs/job-456",
                    "output_path": "/tmp/axon-output/domains/docs.rs/job-456/markdown"
                }
            }),
            Some(Path::new("/tmp/axon-output")),
        );

        assert_eq!(result.output_file_handles.len(), 2);
        assert_eq!(
            result.output_file_handles[0].relative_path,
            "domains/docs.rs/job-456/manifest.jsonl"
        );
        assert_eq!(
            result.output_file_handles[1].relative_path,
            "domains/docs.rs/job-456/markdown"
        );
    }
}
