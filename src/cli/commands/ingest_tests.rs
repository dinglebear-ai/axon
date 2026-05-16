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
    async fn has_active_jobs(&self, _kind: JobKind) -> crate::jobs::backend::BackendResult<bool> {
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
    ) -> Result<Option<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
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
    let temp = tempfile::tempdir()?;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().join("jobs.db");
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
async fn run_ingest_unknown_target_gives_helpful_error() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let temp = tempfile::tempdir()?;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().join("jobs.db");
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
