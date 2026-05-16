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

            async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
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
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
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
            "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json".to_string(),
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
            "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json".to_string(),
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
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

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
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

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
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedLiteRuntime));

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
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CanceledLiteRuntime));

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
