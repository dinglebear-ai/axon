use axon_api::source::{ArtifactHandle, AuthScope, AuthSnapshot, ProviderId, SourceRequest};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use super::*;

fn test_runtime(
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
) -> TargetLocalSourceRuntime {
    test_runtime_with_jobs(vectors, ledger).0
}

fn test_runtime_with_jobs(
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
) -> (TargetLocalSourceRuntime, Arc<FakeJobWatchStore>) {
    let jobs = Arc::new(FakeJobWatchStore::new());
    let runtime = TargetLocalSourceRuntime::new(
        jobs.clone(),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    (runtime, jobs)
}

fn execute_snapshot() -> AuthSnapshot {
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Execute];
    snapshot
}

async fn run_cli_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    auth: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
    policy: &super::tool_auth::ToolExecutionPolicy,
) -> anyhow::Result<IndexCounts> {
    let execution = SourceExecutionContext::inline(SourceRequest::new(input), auth.cloned());
    dispatch_cli_tool(
        runtime,
        input,
        "axon-test",
        "test-owner",
        auth,
        false,
        route,
        &execution,
        policy,
    )
    .await
}

async fn run_mcp_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    auth: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
    policy: &super::tool_auth::ToolExecutionPolicy,
) -> anyhow::Result<IndexCounts> {
    let execution = SourceExecutionContext::inline(SourceRequest::new(input), auth.cloned());
    dispatch_mcp_tool(
        runtime,
        input,
        "axon-test",
        "test-owner",
        auth,
        false,
        route,
        &execution,
        policy,
    )
    .await
}

#[tokio::test]
async fn dispatch_cli_tool_indexes_metadata_without_vectors_or_execution() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());
    let request = SourceRequest::new("cli:rg --help").without_embedding();
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("cli tool should route");

    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("rg");
    let counts = run_cli_tool(&runtime, "cli:rg --help", None, &routed.route, &policy)
        .await
        .expect("cli tool metadata dispatch should succeed");

    assert_eq!(counts.documents_prepared, 1);
    assert!(
        !counts.job_id.0.is_nil(),
        "tool source must use a durable job id"
    );
    assert_eq!(counts.vector_points_written, 0);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
}

#[tokio::test]
async fn dispatch_mcp_tool_indexes_metadata_without_calling_tool() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());
    let request = SourceRequest::new("mcp:labby/search").without_embedding();
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("mcp tool should route");

    let policy = super::tool_auth::ToolExecutionPolicy::test_mcp("labby/search", "/bin/echo");
    let counts = run_mcp_tool(&runtime, "mcp:labby/search", None, &routed.route, &policy)
        .await
        .expect("mcp tool metadata dispatch should succeed");

    assert_eq!(counts.documents_prepared, 1);
    assert!(
        !counts.job_id.0.is_nil(),
        "tool source must use a durable job id"
    );
    assert_eq!(counts.vector_points_written, 0);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
}

#[tokio::test]
async fn dispatch_cli_tool_threads_embed_through_canonical_pipeline() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);
    let request = SourceRequest::new("cli:rg --help");
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let execution = SourceExecutionContext::inline(request, None);
    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("rg");

    let counts = dispatch_cli_tool(
        &runtime,
        "cli:rg --help",
        "axon-test",
        "test-owner",
        None,
        true,
        &routed.route,
        &execution,
        &policy,
    )
    .await
    .expect("tool metadata should use canonical embedding pipeline");

    assert!(counts.chunks_prepared > 0);
    assert!(counts.vector_points_written > 0);
    assert!(!vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_cli_tool_execute_mode_denies_without_execute_scope() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);
    let mut request = SourceRequest::new("cli:/bin/echo hello").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    request.options.values.insert(
        "command_allowlist".to_string(),
        serde_json::json!(["/bin/echo"]),
    );
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("cli tool should route");
    let snapshot = AuthSnapshot::default();

    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("/bin/echo");
    let err = run_cli_tool(
        &runtime,
        "cli:/bin/echo hello",
        Some(&snapshot),
        &routed.route,
        &policy,
    )
    .await
    .expect_err("execute mode must fail closed without execute scope");

    assert!(
        err.to_string().contains("auth.scope_required"),
        "expected execute-scope denial, got: {err:?}"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_cli_tool_ignores_caller_owned_allowlist() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);
    let mut request = SourceRequest::new("cli:/bin/echo hello").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("cli tool should route");
    let snapshot = execute_snapshot();

    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("/bin/false");
    let err = run_cli_tool(
        &runtime,
        "cli:/bin/echo hello",
        Some(&snapshot),
        &routed.route,
        &policy,
    )
    .await
    .expect_err("caller-owned allowlist must not authorize execution");

    assert!(
        err.to_string().contains("tool.command_not_allowlisted"),
        "expected allowlist denial, got: {err:?}"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[test]
fn tool_authorization_uses_server_owned_limits() {
    let mut request = SourceRequest::new("cli:/bin/echo hello").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    request
        .options
        .values
        .insert("timeout_ms".to_string(), serde_json::json!(9_999_999));
    request
        .options
        .values
        .insert("output_cap_bytes".to_string(), serde_json::json!(9_999_999));
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("/bin/echo");
    let authorized = super::tool_auth::authorize_cli_tool_execution(
        "cli:/bin/echo hello",
        Some(&execute_snapshot()),
        &routed.route,
        &policy,
    )
    .unwrap();

    assert_eq!(authorized.policy_metadata["timeout_ms"], 5_000);
    assert_eq!(authorized.policy_metadata["output_cap_bytes"], 65_536);
}

#[tokio::test]
async fn dispatch_cli_tool_execute_captures_redacted_artifact() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let (runtime, jobs) = test_runtime_with_jobs(vectors.clone(), ledger.clone());
    let mut request =
        SourceRequest::new("cli:/bin/echo Authorization:Bearer sk-lane4").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    request.options.values.insert(
        "command_allowlist".to_string(),
        serde_json::json!(["/bin/echo"]),
    );
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("cli tool should route");
    let snapshot = execute_snapshot();

    let policy = super::tool_auth::ToolExecutionPolicy::test_cli("/bin/echo");
    let counts = run_cli_tool(
        &runtime,
        "cli:/bin/echo Authorization:Bearer sk-lane4",
        Some(&snapshot),
        &routed.route,
        &policy,
    )
    .await
    .expect("execute mode should run allowlisted command");

    assert_eq!(counts.documents_prepared, 1);
    assert_eq!(counts.vector_points_written, 0);
    assert_eq!(counts.artifacts.len(), 1);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );

    let artifact = &counts.artifacts[0];
    let stored = runtime
        .artifact_store
        .get(ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri.clone()),
        })
        .await
        .expect("artifact should be readable");
    match stored.content {
        Some(axon_api::source::ContentRef::InlineText { text }) => {
            assert!(text.contains("[REDACTED]") || text.contains("[redacted-secret]"));
            assert!(!text.contains("Authorization"));
            assert!(!text.contains("sk-lane4"));
        }
        other => panic!("expected inline text artifact, got {other:?}"),
    }
    assert_eq!(
        stored.metadata.0.get("redaction_status"),
        Some(&serde_json::json!("redacted"))
    );
    let durable_request = runtime
        .jobs
        .request_json(counts.job_id)
        .await
        .expect("durable request lookup")
        .expect("durable source request");
    let durable_request = serde_json::to_string(&durable_request).unwrap();
    assert!(!durable_request.contains("Authorization"));
    assert!(!durable_request.contains("sk-lane4"));
    let events = jobs.recorded_events(counts.job_id).await;
    let audit = events
        .iter()
        .find(|event| event.message.starts_with("tool execution authorized"))
        .expect("execution audit must be persisted before invocation");
    assert!(!audit.message.contains("/bin/echo"));
    assert!(!audit.message.contains("sk-lane4"));
}

#[tokio::test]
async fn dispatch_mcp_tool_call_requires_caller_command_policy() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);
    let mut request = SourceRequest::new("mcp:labby/search").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("call"));
    request.options.values.insert(
        "mcp_allowlist".to_string(),
        serde_json::json!(["labby/search"]),
    );
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("mcp tool should route");
    let snapshot = execute_snapshot();

    let policy = super::tool_auth::ToolExecutionPolicy::test_mcp_without_caller("labby/search");
    let err = run_mcp_tool(
        &runtime,
        "mcp:labby/search",
        Some(&snapshot),
        &routed.route,
        &policy,
    )
    .await
    .expect_err("call mode requires a caller command");

    assert!(
        err.to_string().contains("mcp.caller_missing"),
        "expected caller policy denial, got: {err:?}"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_mcp_tool_call_captures_artifact() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());
    let mut request = SourceRequest::new("mcp:labby/search").without_embedding();
    request.scope = Some(axon_api::source::SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("call"));
    request.options.values.insert(
        "mcp_allowlist".to_string(),
        serde_json::json!(["labby/search"]),
    );
    request.options.values.insert(
        "mcp_caller_command".to_string(),
        serde_json::json!("/bin/echo"),
    );
    request.options.values.insert(
        "mcp_caller_allowlist".to_string(),
        serde_json::json!(["/bin/echo"]),
    );
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("mcp tool should route");
    let snapshot = execute_snapshot();

    let policy = super::tool_auth::ToolExecutionPolicy::test_mcp("labby/search", "/bin/echo");
    let counts = run_mcp_tool(
        &runtime,
        "mcp:labby/search",
        Some(&snapshot),
        &routed.route,
        &policy,
    )
    .await
    .expect("call mode should invoke allowlisted caller");

    assert_eq!(counts.documents_prepared, 1);
    assert_eq!(counts.vector_points_written, 0);
    assert_eq!(counts.artifacts.len(), 1);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
}
