use crate::crates::cli::commands::CommandFuture;
use crate::crates::cli::commands::ingest_common;
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{accent, muted, primary};
use crate::crates::jobs::ingest::IngestSource;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::ingest as ingest_service;
use crate::crates::services::types::StartDisposition;
use std::error::Error;

pub fn run_ingest<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
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
            let result = enqueue_ingest_job(cfg, source, service_context).await;
            if result.is_ok() {
                log_info(&format!(
                    "job_enqueued command=ingest queue={}",
                    cfg.ingest_queue
                ));
            }
            return result;
        }

        ingest_common::run_ingest_sync(cfg, source).await
    })
}

async fn enqueue_ingest_job(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome = ingest_service::ingest_start_with_context(cfg, source, service_context).await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"job_id": job_id, "status": status})
        );
    } else {
        println!("  {} {}", primary("Ingest Job"), accent(&job_id));
        if outcome.disposition == StartDisposition::Completed {
            println!("  {}", muted("Lite mode completed the ingest in-process."));
        }
        println!("Job ID: {job_id}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::CommandKind;
    use crate::crates::jobs::backend::{
        BackendResult, JobBackend, JobId, JobKind, JobPayload, JobStatusRow, JobSummary,
    };
    use crate::crates::jobs::common::test_config;
    use crate::crates::services::context::ServiceContext;
    use async_trait::async_trait;
    use std::sync::Arc;

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
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await
            .expect("service context")
            .with_job_backend(noop_backend());
        let err = run_ingest(&cfg, &ctx)
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
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await
            .expect("service context")
            .with_job_backend(noop_backend());
        let err = run_ingest(&cfg, &ctx)
            .await
            .expect_err("expected classification error");
        assert!(
            err.to_string().contains("cannot determine ingest source"),
            "unexpected error: {err}"
        );
    }
}
