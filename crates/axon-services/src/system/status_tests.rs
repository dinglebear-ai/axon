use super::*;
use crate::runtime::ServiceJobRuntime;
use async_trait::async_trait;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use axon_source_ledger::{SourceKind, SourcePhase, SourceStatus};
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

#[tokio::test]
async fn source_status_redacts_headers_and_local_paths() {
    for sensitive in [
        "Authorization: Bearer secret",
        "authorization: Bearer secret",
        "Cookie: sid=abc",
        "cookie: sid=abc",
        "failed at /home/jmagar/private/repo",
        "failed at C:\\Users\\Jacob\\private\\repo",
    ] {
        let status = SourceStatus {
            source_id: "local-code".to_string(),
            source_kind: SourceKind::LocalCode,
            phase: SourcePhase::BackingOff,
            committed_generation: 7,
            active_generation: Some(8),
            backoff_until_ms: Some(123_456),
            last_error: Some(sensitive.to_string()),
            cleanup_debt_count: 2,
            updated_at_ms: 123_400,
        };

        let body = serde_json::to_string(&status).unwrap();

        assert!(!body.contains("Bearer secret"), "{body}");
        assert!(!body.contains("sid=abc"), "{body}");
        assert!(!body.contains("/home/"), "{body}");
        assert!(!body.contains("C:\\Users"), "{body}");
        assert!(body.contains("[redacted]"), "{body}");
    }
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

struct HealthyRuntime;

#[async_trait]
impl ServiceJobRuntime for HealthyRuntime {
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

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

#[tokio::test]
#[serial_test::serial(sqlite_runtime_health)]
async fn full_status_marks_count_failures_degraded() {
    axon_jobs::store::reset_sqlite_runtime_health_for_tests();
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        sqlite_path: tmp.path().join("jobs.db"),
        ..Default::default()
    };
    let ctx = ServiceContext::from_runtime(Arc::new(cfg), Arc::new(CountFailRuntime));

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

/// Resets the process-global SQLite runtime health on drop, so the recorded
/// IOERR below is cleared even if an assertion panics (unwind), not just on the
/// success path. Without this, a panic would leak the global error into other
/// tests in this binary.
struct ResetSqliteRuntimeHealthOnDrop;

impl Drop for ResetSqliteRuntimeHealthOnDrop {
    fn drop(&mut self) {
        axon_jobs::store::reset_sqlite_runtime_health_for_tests();
    }
}

#[tokio::test]
#[serial_test::serial(sqlite_runtime_health)]
async fn full_status_includes_sqlite_diagnostics_and_degrades_on_runtime_ioerr() {
    let _reset = ResetSqliteRuntimeHealthOnDrop;
    axon_jobs::store::reset_sqlite_runtime_health_for_tests();
    axon_jobs::store::record_sqlite_runtime_error(
        "error returned from database: (code: 522) disk I/O error",
    );
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        sqlite_path: tmp.path().join("jobs.db"),
        ..Default::default()
    };
    let ctx = ServiceContext::from_runtime(Arc::new(cfg), Arc::new(HealthyRuntime));

    let result = full_status(&ctx).await.expect("status");

    assert!(result.degraded);
    assert_eq!(result.payload["sqlite"]["runtime_ioerr_count"], 1);
    assert_eq!(result.payload["sqlite"]["ok"], false);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("SQLite runtime IOERR"))
    );

    // The `_reset` guard above clears the recorded global IOERR on scope exit
    // (including unwind), so it cannot leak into other tests in this binary.
}
