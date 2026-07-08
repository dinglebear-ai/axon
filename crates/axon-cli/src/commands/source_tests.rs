use super::*;
use axon_core::config::CommandKind;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use axon_services::runtime::ServiceJobRuntime;
use axon_services::types::ServiceJob;
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
/// local-source runtime, so `run_source` exercises the degraded "no data plane"
/// path and returns the shim's nonzero-exit error.
fn context_without_data_plane() -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::test_default()), Arc::new(NoopRuntime))
}

fn source_cfg(path: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Source;
    cfg.positional = vec![path.to_string()];
    cfg
}

#[tokio::test]
async fn run_source_requires_data_plane_runtime() {
    // A real, existing local path so classification passes and the
    // missing-runtime degraded path is what surfaces as the error.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let cfg = source_cfg(&dir.path().to_string_lossy());
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("requires a running data plane"),
        "expected data-plane guard error, got: {msg}"
    );
}

#[tokio::test]
async fn run_source_git_url_requires_data_plane_runtime() {
    let cfg = source_cfg("https://github.com/jmagar/axon.git");
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("requires a running data plane"),
        "expected data-plane guard error for git input, got: {msg}"
    );
}

#[tokio::test]
async fn run_source_web_url_requires_data_plane_runtime() {
    let cfg = source_cfg("https://docs.example.com/guide");
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("requires a running data plane"),
        "expected data-plane guard error for web input, got: {msg}"
    );
}

#[tokio::test]
async fn run_source_rejects_missing_positional_path() {
    let mut cfg = source_cfg("");
    cfg.positional = vec![];
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string().contains(
            "requires a local path, git repository URL, feed URL, youtube target, reddit target"
        ),
        "expected missing-path error, got: {err}"
    );
}

#[tokio::test]
async fn run_source_rejects_unsupported_input() {
    let cfg = source_cfg("not-a-path-or-url");
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported source input"),
        "expected unsupported-input error, got: {msg}"
    );
}
