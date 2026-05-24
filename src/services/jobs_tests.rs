use super::*;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::services::runtime::ServiceJobRuntime;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct CaptureRuntime {
    seen_filters: Mutex<Vec<Option<String>>>,
}

#[async_trait]
impl ServiceJobRuntime for CaptureRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        Err("not implemented".into())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Err("not implemented".into())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        self.seen_filters
            .lock()
            .expect("lock")
            .push(source_filter.map(str::to_string));
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<crate::jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

#[tokio::test]
async fn list_ingest_jobs_delegates_source_filter_to_runtime() {
    let cfg = Arc::new(crate::core::config::Config::default());
    let runtime = Arc::new(CaptureRuntime {
        seen_filters: Mutex::new(Vec::new()),
    });
    let ctx = ServiceContext::from_runtime(cfg, runtime.clone());

    let jobs = list_ingest_jobs(&ctx, Some("sessions"), 50, 0)
        .await
        .expect("list should succeed");
    assert!(jobs.is_empty());
    assert_eq!(
        runtime.seen_filters.lock().expect("lock").as_slice(),
        &[Some("sessions".to_string())]
    );
}
