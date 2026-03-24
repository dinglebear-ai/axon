use crate::crates::cli::commands::ingest_common;
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::jobs::backend::JobBackend;
use crate::crates::services::ingest as ingest_service;
use std::error::Error;
use std::sync::Arc;

pub async fn run_ingest(cfg: &Config, backend: &Arc<dyn JobBackend>) -> Result<(), Box<dyn Error>> {
    let _ = backend; // backend reserved for future lite-mode dispatch
    if ingest_common::maybe_handle_ingest_subcommand(cfg, "ingest").await? {
        return Ok(());
    }
    if cfg.lite_mode && cfg.positional.first().map(|s| s.as_str()) == Some("worker") {
        println!("Lite mode: workers run in-process automatically. No separate worker needed.");
        return Ok(());
    }

    let target = cfg
        .positional
        .first()
        .cloned()
        .ok_or("ingest requires a target: GitHub slug (owner/repo), YouTube URL or @handle, or Reddit subreddit (r/name) or URL")?;

    log_info(&format!("command=ingest target={target}"));
    let source = ingest_service::classify_target(&target, cfg.github_include_source)?;

    if !cfg.wait {
        let result = ingest_common::enqueue_ingest_job(cfg, source).await;
        if result.is_ok() {
            log_info(&format!(
                "job_enqueued command=ingest queue={}",
                cfg.ingest_queue
            ));
        }
        return result;
    }

    ingest_common::run_ingest_sync(cfg, source).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::CommandKind;
    use crate::crates::jobs::backend::{
        BackendResult, JobId, JobKind, JobPayload, JobStatusRow, JobSummary,
    };
    use crate::crates::jobs::common::test_config;
    use async_trait::async_trait;

    /// Minimal no-op backend for unit tests that do not exercise the job path.
    struct NoopBackend;

    #[async_trait]
    impl JobBackend for NoopBackend {
        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<JobId> {
            unimplemented!("NoopBackend::enqueue not used in these tests")
        }
        async fn job_status(
            &self,
            _id: JobId,
            _kind: JobKind,
        ) -> BackendResult<Option<JobStatusRow>> {
            unimplemented!()
        }
        async fn cancel_job(&self, _id: JobId, _kind: JobKind) -> BackendResult<bool> {
            unimplemented!()
        }
        async fn list_jobs(&self, _kind: JobKind) -> BackendResult<Vec<JobSummary>> {
            unimplemented!()
        }
        async fn cleanup_jobs(&self, _kind: JobKind) -> BackendResult<u64> {
            unimplemented!()
        }
        async fn clear_jobs(&self, _kind: JobKind) -> BackendResult<u64> {
            unimplemented!()
        }
        async fn job_errors(&self, _id: JobId, _kind: JobKind) -> BackendResult<Option<String>> {
            unimplemented!()
        }
    }

    fn noop_backend() -> Arc<dyn JobBackend> {
        Arc::new(NoopBackend)
    }

    #[tokio::test]
    async fn run_ingest_requires_target() {
        let mut cfg = test_config("");
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec![];
        let backend = noop_backend();
        let err = run_ingest(&cfg, &backend)
            .await
            .expect_err("expected missing target error");
        assert!(
            err.to_string().contains("ingest requires a target"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn run_ingest_unknown_target_gives_helpful_error() {
        let mut cfg = test_config("");
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec!["not-a-target".to_string()];
        let backend = noop_backend();
        let err = run_ingest(&cfg, &backend)
            .await
            .expect_err("expected classification error");
        assert!(
            err.to_string().contains("cannot determine ingest source"),
            "unexpected error: {err}"
        );
    }
}
