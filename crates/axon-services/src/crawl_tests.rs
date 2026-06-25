use super::{crawl_start_with_context, map_crawl_start_result, predict_crawl_output_dir};
use crate::context::ServiceContext;
use crate::runtime::ServiceJobRuntime;
use crate::types::ServiceJob;
use crate::types::{ExecutionMode, StartDisposition};
use async_trait::async_trait;
use axon_core::config::Config;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

fn test_config(start_url: &str) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.start_url = start_url.to_string();
    cfg
}

struct CompletedRuntime;
struct CanceledRuntime;
struct CrawlWithDependentEmbedRuntime {
    crawl_job_id: Uuid,
    embed_job_id: Uuid,
    wait_calls: Mutex<Vec<(Uuid, JobKind)>>,
}
struct CapturingRuntime {
    payloads: Mutex<Vec<JobPayload>>,
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
            ) -> Result<Vec<crate::types::ServiceJob>, Box<dyn std::error::Error + Send + Sync>>
            {
                Ok(vec![])
            }

            async fn job_status(
                &self,
                _kind: JobKind,
                _id: Uuid,
            ) -> Result<Option<crate::types::ServiceJob>, Box<dyn std::error::Error + Send + Sync>>
            {
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

            async fn count_jobs_by_status(
                &self,
                _kind: JobKind,
            ) -> Result<
                std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
                Box<dyn std::error::Error + Send + Sync>,
            > {
                Ok(std::collections::HashMap::new())
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

impl_noop_runtime_for!(CompletedRuntime, Ok("completed".to_string()), Ok(None));
impl_noop_runtime_for!(
    CanceledRuntime,
    Ok("canceled".to_string()),
    Ok(Some("user canceled".to_string()))
);

#[async_trait]
impl ServiceJobRuntime for CapturingRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.payloads.lock().unwrap().push(payload);
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
    ) -> Result<Vec<ServiceJob>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn std::error::Error + Send + Sync>> {
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Ok("completed".to_string())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }
}

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
            progress_json: None,
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
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
        result.predicted_artifact_handles[0].relative_path(),
        "domains/docs.rs/job-123/manifest.jsonl"
    );
    assert_eq!(
        result.predicted_artifact_handles[0].job_id(),
        Some("job-123")
    );
    assert_eq!(
        result.predicted_artifact_handles[0].url(),
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
async fn crawl_start_with_context_completes_with_sqlite_backend()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cfg = test_config("https://docs.rs");
    cfg.wait = true;
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedRuntime));

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
async fn crawl_start_with_context_rejects_empty_urls_with_sqlite_backend() {
    let cfg = test_config("https://docs.rs");
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedRuntime));

    let err = crawl_start_with_context(&cfg, &[], &ctx, None)
        .await
        .expect_err("empty urls must fail");
    assert!(err.to_string().contains("No URLs provided"));
}

#[tokio::test]
async fn crawl_start_with_context_enqueues_without_blocking_with_sqlite_backend()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cfg = test_config("https://docs.rs");
    cfg.wait = false;
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CompletedRuntime));

    let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
        .await
        .map_err(|e| e.to_string())?;

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
    assert_eq!(outcome.result.jobs.len(), 1);
    Ok(())
}

#[tokio::test]
async fn crawl_start_with_context_surfaces_canceled_jobs_with_sqlite_backend() {
    let mut cfg = test_config("https://docs.rs");
    cfg.wait = true;
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(CanceledRuntime));

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
        result.output_file_handles[0].relative_path(),
        "manifest.jsonl"
    );
    assert_eq!(result.output_file_handles[0].job_id(), Some("job-123"));
    assert_eq!(result.output_file_handles[0].url(), Some("https://docs.rs"));
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
        result.output_file_handles[0].relative_path(),
        "domains/docs.rs/job-456/manifest.jsonl"
    );
    assert_eq!(
        result.output_file_handles[1].relative_path(),
        "domains/docs.rs/job-456/markdown"
    );
}

#[test]
fn resolve_crawl_max_pages_default_and_cap() {
    use super::{DEFAULT_CRAWL_MAX_PAGES, resolve_crawl_max_pages};
    // unspecified (0) → default
    assert_eq!(resolve_crawl_max_pages(0, false), DEFAULT_CRAWL_MAX_PAGES);
    // within bounds → unchanged
    assert_eq!(resolve_crawl_max_pages(120, false), 120);
    assert_eq!(
        resolve_crawl_max_pages(DEFAULT_CRAWL_MAX_PAGES, false),
        DEFAULT_CRAWL_MAX_PAGES
    );
    // over the cap → clamped
    assert_eq!(
        resolve_crawl_max_pages(50_000, false),
        DEFAULT_CRAWL_MAX_PAGES
    );
    // operator override passes through untouched, including 0 (uncapped)
    assert_eq!(resolve_crawl_max_pages(0, true), 0);
    assert_eq!(resolve_crawl_max_pages(50_000, true), 50_000);
}

#[tokio::test]
async fn crawl_start_snapshots_effective_max_pages_at_enqueue_boundary()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    async fn captured_max_pages(
        requested: u32,
        allow_unbounded: bool,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let mut cfg = test_config("https://docs.rs");
        cfg.max_pages = requested;
        cfg.allow_unbounded_broad_crawl = allow_unbounded;
        let runtime = Arc::new(CapturingRuntime {
            payloads: Mutex::new(Vec::new()),
        });
        let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

        crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None)
            .await
            .map_err(|e| e.to_string())?;

        let payloads = runtime.payloads.lock().unwrap();
        let [JobPayload::Crawl { config_json, .. }] = payloads.as_slice() else {
            panic!("expected exactly one captured crawl payload");
        };
        let snapshot: serde_json::Value =
            serde_json::from_str(config_json).expect("config_json is valid JSON");
        Ok(snapshot
            .pointer("/config/max_pages")
            .and_then(serde_json::Value::as_u64)
            .expect("crawl snapshot has max_pages") as u32)
    }

    assert_eq!(captured_max_pages(0, false).await?, 5_000);
    assert_eq!(captured_max_pages(50_000, false).await?, 5_000);
    assert_eq!(captured_max_pages(0, true).await?, 0);
    assert_eq!(captured_max_pages(50_000, true).await?, 50_000);
    Ok(())
}
