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
    TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

fn execute_snapshot() -> AuthSnapshot {
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Execute];
    snapshot
}

#[tokio::test]
async fn dispatch_cli_tool_indexes_metadata_without_vectors_or_execution() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());
    let request = SourceRequest::new("cli:rg --help").without_embedding();
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("cli tool should route");

    let counts = dispatch_cli_tool(&runtime, "cli:rg --help", "test-owner", None, &routed.route)
        .await
        .expect("cli tool metadata dispatch should succeed");

    assert_eq!(counts.documents_prepared, 1);
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

    let counts = dispatch_mcp_tool(
        &runtime,
        "mcp:labby/search",
        "test-owner",
        None,
        &routed.route,
    )
    .await
    .expect("mcp tool metadata dispatch should succeed");

    assert_eq!(counts.documents_prepared, 1);
    assert_eq!(counts.vector_points_written, 0);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
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

    let err = dispatch_cli_tool(
        &runtime,
        "cli:/bin/echo hello",
        "test-owner",
        Some(&snapshot),
        &routed.route,
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
async fn dispatch_cli_tool_execute_mode_denies_missing_allowlist() {
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

    let err = dispatch_cli_tool(
        &runtime,
        "cli:/bin/echo hello",
        "test-owner",
        Some(&snapshot),
        &routed.route,
    )
    .await
    .expect_err("execute mode requires allowlist policy");

    assert!(
        err.to_string().contains("tool.command_not_allowlisted"),
        "expected allowlist denial, got: {err:?}"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_cli_tool_execute_captures_redacted_artifact() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());
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

    let counts = dispatch_cli_tool(
        &runtime,
        "cli:/bin/echo Authorization:Bearer sk-lane4",
        "test-owner",
        Some(&snapshot),
        &routed.route,
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

    let err = dispatch_mcp_tool(
        &runtime,
        "mcp:labby/search",
        "test-owner",
        Some(&snapshot),
        &routed.route,
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

    let counts = dispatch_mcp_tool(
        &runtime,
        "mcp:labby/search",
        "test-owner",
        Some(&snapshot),
        &routed.route,
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
