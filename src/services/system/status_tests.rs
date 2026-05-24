use super::*;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::services::runtime::ServiceJobRuntime;
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn status_payload_includes_expected_keys() {
    let payload = build_status_payload(&[], &[], &[], &[], &StatusTotals::default());
    assert!(payload.get("local_crawl_jobs").is_some());
    assert!(payload.get("local_ingest_jobs").is_some());
    assert!(payload.get("totals").is_some());
    assert_eq!(
        payload.get("degraded").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        payload
            .get("errors")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(0)
    );
}

struct CountFailRuntime;

#[async_trait]
impl ServiceJobRuntime for CountFailRuntime {
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

    async fn count_jobs(&self, kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        match kind {
            JobKind::Crawl => Err("database is locked".into()),
            _ => Ok(7),
        }
    }
}

#[tokio::test]
async fn full_status_marks_count_failures_degraded() {
    let ctx = ServiceContext::from_runtime(Arc::new(Config::default()), Arc::new(CountFailRuntime));

    let result = full_status(&ctx)
        .await
        .expect("status should degrade, not fail");

    assert!(result.degraded);
    assert_eq!(result.totals.crawl, 0);
    assert_eq!(result.totals.extract, 7);
    assert_eq!(
        result
            .payload
            .get("degraded")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    let errors = result
        .payload
        .get("errors")
        .and_then(|value| value.as_array())
        .expect("errors array");
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .as_str()
            .expect("string error")
            .contains("database is locked")
    );
}
