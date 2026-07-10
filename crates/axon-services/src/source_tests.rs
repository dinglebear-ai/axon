use super::*;
use crate::runtime::ServiceJobRuntime;
use crate::source::classify::SourceInputKind;
use crate::types::ServiceJob;
use axon_api::source::{LifecycleStatus, SourceKind};
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
async fn source_routing_resolves_web_before_data_plane() {
    let mut request = SourceRequest::new("example.com");
    request.scope = Some(SourceScope::Map);

    let routed =
        routing::resolve_source_route(&request).expect("scheme-less web source should route");

    assert_eq!(routed.kind, SourceInputKind::Web);
    assert_eq!(routed.route.adapter.name, "web");
    assert_eq!(routed.route.scope, SourceScope::Map);
    assert_eq!(routed.route.source.canonical_uri, "https://example.com/");
}

#[tokio::test]
async fn source_routing_rejects_unsupported_scope_before_data_plane() {
    let mut request = SourceRequest::new("crates:serde");
    request.scope = Some(SourceScope::Subreddit);

    let err = routing::resolve_source_route(&request)
        .expect_err("registry source must reject reddit scope before acquisition");

    assert_eq!(err.code.0, "source.scope.unsupported");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
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
async fn index_source_rejects_bad_scope_before_data_plane() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("crates:serde");
    request.scope = Some(SourceScope::Subreddit);

    let result = index_source(request, &ctx)
        .await
        .expect("route failure is returned as a failed SourceResult");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "source.scope.unsupported"),
        "expected route scope warning, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_uses_routed_scope_without_data_plane() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("example.com");
    request.scope = Some(SourceScope::Map);
    request.embed = false;

    let result = index_source(request, &ctx)
        .await
        .expect("missing data plane returns a degraded result");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Web);
    assert_eq!(result.scope, SourceScope::Map);
    assert_eq!(result.adapter.name, "web");
    assert_eq!(result.canonical_uri, "https://example.com/");
}

#[tokio::test]
async fn index_source_web_map_scope_is_reported_without_falling_back_to_site() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("https://example.com/docs");
    request.intent = axon_api::source::SourceIntent::Map;
    request.scope = Some(SourceScope::Map);
    request.embed = false;

    let result = index_source(request, &ctx)
        .await
        .expect("missing data plane returns degraded result");

    assert_eq!(result.source_kind, SourceKind::Web);
    assert_eq!(result.scope, SourceScope::Map);
    assert_eq!(result.canonical_uri, "https://example.com/docs");
}

#[tokio::test]
async fn source_routing_covers_phase_4_input_families() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let local_path = temp.path().to_string_lossy().to_string();
    let cases = vec![
        (
            SourceRequest::new(local_path),
            SourceKind::Local,
            SourceScope::Directory,
            "local",
        ),
        (
            SourceRequest::new("https://github.com/jmagar/axon"),
            SourceKind::Git,
            SourceScope::Repo,
            "github",
        ),
        (
            SourceRequest::new("npm:left-pad"),
            SourceKind::Registry,
            SourceScope::Package,
            "npm",
        ),
        (
            SourceRequest::new("r/rust"),
            SourceKind::Reddit,
            SourceScope::Subreddit,
            "reddit",
        ),
        (
            SourceRequest::new("https://youtube.com/watch?v=dQw4w9WgXcQ"),
            SourceKind::Youtube,
            SourceScope::Video,
            "youtube",
        ),
        (
            SourceRequest::new("feed:https://example.com/feed.xml"),
            SourceKind::Feed,
            SourceScope::Feed,
            "feed",
        ),
        (
            SourceRequest::new("session:claude:/tmp/session.jsonl"),
            SourceKind::Session,
            SourceScope::Thread,
            "session",
        ),
        (
            SourceRequest::new("mcp:context7/resolve-library-id"),
            SourceKind::McpTool,
            SourceScope::Tool,
            "mcp",
        ),
        (
            SourceRequest::new("cli:rg"),
            SourceKind::CliTool,
            SourceScope::Tool,
            "cli",
        ),
    ];

    for (request, expected_kind, expected_scope, expected_adapter) in cases {
        let routed = routing::resolve_source_route(&request)
            .unwrap_or_else(|err| panic!("{} should route: {err}", request.source));
        assert_eq!(
            routed.route.source.source_kind, expected_kind,
            "{}",
            request.source
        );
        assert_eq!(routed.route.scope, expected_scope, "{}", request.source);
        assert_eq!(
            routed.route.adapter.name, expected_adapter,
            "{}",
            request.source
        );
    }
}

#[tokio::test]
async fn index_source_reports_unsupported_dispatch_for_tool_sources() {
    let ctx = context_without_data_plane();
    let result = index_source(SourceRequest::new("cli:rg"), &ctx)
        .await
        .expect("unsupported dispatch is represented as failed SourceResult");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "source.route.unsupported_dispatch"),
        "expected unsupported dispatch warning, got: {:?}",
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
            .any(|w| w.code == "source.resolve.unsupported"),
        "expected unsupported-input route warning, got: {:?}",
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
