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
/// local-source runtime, so it exercises the "requires data plane" guard.
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
    // A real, existing local path so the local-path check passes and the
    // missing-runtime guard is what fires.
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
    // A git URL that is not a local path routes to the git branch, which still
    // requires the data plane — assert the shared guard fires there too.
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
async fn run_source_rejects_missing_positional_path() {
    let mut cfg = source_cfg("");
    cfg.positional = vec![];
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("requires a local path, git repository URL, or web URL"),
        "expected missing-path error, got: {err}"
    );
}

#[tokio::test]
async fn run_source_web_url_requires_data_plane_runtime() {
    // A plain http/https URL that is NOT a git target routes to the web branch,
    // which requires the data plane BEFORE any crawl work — assert the guard
    // fires there (no network access in this test).
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
async fn run_source_rejects_unsupported_input() {
    // A plain word is neither a local path nor a git URL.
    let cfg = source_cfg("not-a-path-or-url");
    let ctx = context_without_data_plane();

    let err = run_source(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("local paths, git repository URLs, and web URLs"),
        "expected unsupported-input error, got: {msg}"
    );
    assert!(
        msg.contains("P10 follow-up"),
        "error should name the P10 follow-up, got: {msg}"
    );
}

#[tokio::test]
async fn classify_source_input_prefers_existing_local_path() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().to_string_lossy().to_string();
    assert_eq!(classify_source_input(&path).await, SourceInputKind::Local);
}

#[tokio::test]
async fn classify_source_input_detects_git_url() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon.git").await,
        SourceInputKind::Git
    );
    assert_eq!(
        classify_source_input("https://gitlab.com/owner/repo").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_detects_web_url() {
    // Plain http/https URLs that are not git targets classify as Web.
    assert_eq!(
        classify_source_input("https://docs.example.com/guide").await,
        SourceInputKind::Web
    );
    assert_eq!(
        classify_source_input("http://example.com").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_git_url_beats_web_url() {
    // A GitHub URL is an http URL, but git classification runs first, so it must
    // route to Git, not Web.
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_dot_git_on_unknown_host_is_git() {
    // A `.git` suffix is an explicit git signal even on a host we don't
    // recognize, so it routes to Git rather than the web crawl.
    assert_eq!(
        classify_source_input("https://example.com/team/repo.git").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_plain_https_path_is_web_not_git() {
    // A plain docs URL parses as a generic git target under the permissive
    // parser, but has no git signal, so `axon source` routes it to the web
    // crawl branch.
    assert_eq!(
        classify_source_input("https://docs.example.com/team/guide").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_rejects_plain_word() {
    assert_eq!(
        classify_source_input("just-a-word").await,
        SourceInputKind::Unsupported
    );
    // Non-http schemes are not web sources.
    assert_eq!(
        classify_source_input("ftp://example.com/file").await,
        SourceInputKind::Unsupported
    );
}
