use crate::cli::commands::CommandFuture;
use crate::cli::commands::ingest_common;
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, muted, primary};
use crate::services::context::ServiceContext;
use crate::services::ingest::{self as ingest_service, IngestSource};
use crate::services::types::StartDisposition;
use std::error::Error;

pub fn run_ingest<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if ingest_common::maybe_handle_ingest_subcommand(cfg, service_context, "ingest").await? {
            return Ok(());
        }

        let target = cfg.positional.first().cloned().ok_or(
            "ingest requires a target. Examples:\n\
                 \n\
                 GitHub:   axon ingest owner/repo\n\
                 Reddit:   axon ingest r/rust\n\
                           axon ingest https://reddit.com/r/rust/comments/...\n\
                 YouTube:  axon ingest https://youtube.com/watch?v=...\n\
                           axon ingest @channelname\n\
                 \n\
                 Run 'axon ingest --help' for full usage.",
        )?;

        log_info(&format!("command=ingest target={target}"));
        let source = ingest_service::classify_target(&target, cfg.github_include_source)?;

        if !cfg.wait {
            let result = enqueue_ingest_job(cfg, source, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=ingest");
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
            serde_json::to_string_pretty(&serde_json::json!({"job_id": job_id, "status": status}))?
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
    use crate::core::config::{CommandKind, Config};
    use crate::jobs::backend::JobKind;
    use crate::services::context::ServiceContext;
    use crate::services::runtime::ServiceJobRuntime;
    use async_trait::async_trait;
    use std::error::Error;
    use std::sync::Arc;

    struct NoopRuntime;

    #[async_trait]
    impl ServiceJobRuntime for NoopRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }
        async fn enqueue(
            &self,
            _payload: crate::jobs::backend::JobPayload,
        ) -> crate::jobs::backend::BackendResult<uuid::Uuid> {
            unimplemented!()
        }
        async fn wait_for_job(
            &self,
            _id: uuid::Uuid,
            _kind: JobKind,
        ) -> crate::jobs::backend::BackendResult<String> {
            unimplemented!()
        }
        async fn job_errors(
            &self,
            _id: uuid::Uuid,
            _kind: JobKind,
        ) -> crate::jobs::backend::BackendResult<Option<String>> {
            unimplemented!()
        }
        async fn has_active_jobs(
            &self,
            _kind: JobKind,
        ) -> crate::jobs::backend::BackendResult<bool> {
            unimplemented!()
        }
        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
        async fn job_status(
            &self,
            _kind: JobKind,
            _id: uuid::Uuid,
        ) -> Result<Option<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>>
        {
            unimplemented!()
        }
        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: uuid::Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
        async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn run_ingest_requires_target() -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec![];
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await?
            .with_jobs_runtime(Arc::new(NoopRuntime));
        let err = run_ingest(&cfg, &ctx)
            .await
            .expect_err("expected missing target error");
        assert!(
            err.to_string().contains("ingest requires a target"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn run_ingest_unknown_target_gives_helpful_error()
    -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Ingest;
        cfg.positional = vec!["not-a-target".to_string()];
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await?
            .with_jobs_runtime(Arc::new(NoopRuntime));
        let err = run_ingest(&cfg, &ctx)
            .await
            .expect_err("expected classification error");
        assert!(
            err.to_string().contains("cannot determine ingest source"),
            "unexpected error: {err}"
        );
        Ok(())
    }
}
