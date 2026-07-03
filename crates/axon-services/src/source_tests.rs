use super::*;
use crate::runtime::ServiceJobRuntime;
use crate::types::ServiceJob;
use axon_core::config::Config;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use std::error::Error as StdError;
use std::sync::Arc;
use uuid::Uuid;

struct NoopRuntime;

#[async_trait::async_trait]
impl ServiceJobRuntime for NoopRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Ok("completed".to_string())
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
    ) -> Result<Vec<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn StdError + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn StdError + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

/// A `ServiceContext` built with `from_runtime` never attaches a target
/// local-source runtime, so it exercises the "no data plane" degraded path.
fn context_without_data_plane() -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::test_default()), Arc::new(NoopRuntime))
}

#[tokio::test]
async fn index_source_empty_input_is_unsupported() {
    let ctx = context_without_data_plane();
    let result = index_source(SourceRequest::new("   "), &ctx)
        .await
        .expect("empty input returns a degraded result, not Err");
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.code == "unsupported_source"),
        "expected unsupported_source warning, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_unsupported_input_is_unsupported() {
    let ctx = context_without_data_plane();
    let result = index_source(SourceRequest::new("not-a-path-or-url"), &ctx)
        .await
        .expect("unsupported input returns a degraded result, not Err");
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Web);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.message.contains("is none of these")),
        "expected unsupported-input message, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_local_without_data_plane_is_degraded() {
    // An existing local path classifies as Local; without a data plane the
    // orchestrator returns a Failed SourceResult with the data-plane warning.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let ctx = context_without_data_plane();
    let result = index_source(
        SourceRequest::new(dir.path().to_string_lossy().to_string()),
        &ctx,
    )
    .await
    .expect("missing data plane returns a degraded result, not Err");
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Local);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.code == "data_plane_unconfigured"),
        "expected data_plane_unconfigured warning, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_git_without_data_plane_is_degraded() {
    let ctx = context_without_data_plane();
    let result = index_source(
        SourceRequest::new("https://github.com/jmagar/axon.git"),
        &ctx,
    )
    .await
    .expect("missing data plane returns a degraded result, not Err");
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Git);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.code == "data_plane_unconfigured"),
        "expected data_plane_unconfigured warning for git, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_web_without_data_plane_is_degraded() {
    let ctx = context_without_data_plane();
    let result = index_source(SourceRequest::new("https://docs.example.com/guide"), &ctx)
        .await
        .expect("missing data plane returns a degraded result, not Err");
    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Web);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.code == "data_plane_unconfigured"),
        "expected data_plane_unconfigured warning for web, got: {:?}",
        result.warnings
    );
}
