use super::*;
use crate::core::config::Config;
use crate::jobs::backend::BackendResult;
use crate::services::context::ServiceContext;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

struct CaptureRuntime {
    payloads: Mutex<Vec<JobPayload>>,
}

#[async_trait]
impl ServiceJobRuntime for CaptureRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.payloads.lock().expect("lock").push(payload);
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        panic!("--wait false embed start must enqueue without waiting")
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        panic!("--wait false embed start must not drain the queue")
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<crate::services::types::ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }
}

#[tokio::test]
async fn embed_start_with_context_enqueues_without_blocking_when_wait_false()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let mut cfg = Config::test_default();
    cfg.wait = false;
    let runtime = Arc::new(CaptureRuntime {
        payloads: Mutex::new(Vec::new()),
    });
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

    let outcome = embed_start_with_context(&cfg, "./README.md", &ctx, None, None)
        .await
        .map_err(|e| e.to_string())?;

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
    assert_eq!(runtime.payloads.lock().expect("lock").len(), 1);
    Ok(())
}
