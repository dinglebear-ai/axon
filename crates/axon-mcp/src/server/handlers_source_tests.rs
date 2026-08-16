use super::*;
use axon_api::source::{AuthMode, CallerContext, TransportKind, Visibility};

/// Build an `AuthSnapshot` as if a Mounted-mode MCP caller presented exactly
/// `scopes` (e.g. `&["axon:write"]`). Mirrors how `call_tool` builds the real
/// snapshot from a resolved `AuthContext` (`server.rs`), so these tests
/// exercise the same conversion path `enforce_source_safety_scope` sees in
/// production.
fn snapshot_with_scopes(scopes: &[&str]) -> AuthSnapshot {
    AuthSnapshot::from_caller(
        &CallerContext {
            caller_id: Some("tester".to_string()),
            transport: TransportKind::Mcp,
            trusted_local: false,
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            visibility_ceiling: Visibility::Internal,
            auth_mode: AuthMode::Oauth,
            token_id: None,
            display_name: None,
        },
        Visibility::Internal,
        "test",
    )
}

/// A local-filesystem source is refused for a Mounted caller holding only the
/// broad `axon:write` scope — this is the audit finding (bead
/// `axon_rust-ldozg`): previously `handle_source` had no per-target scope
/// upgrade at all, so this request would have proceeded straight into
/// `index_source_with_auth` instead of being denied here, before any service
/// context or data-plane work.
#[tokio::test]
async fn source_local_path_denied_without_local_scope() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let snapshot = snapshot_with_scopes(&["axon:write"]);
    let result = enforce_source_safety_scope(&tmp.path().to_string_lossy(), Some(&snapshot)).await;

    let err = result.expect_err("local source without axon:local must be refused");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_REQUEST);
    assert!(
        err.message.to_lowercase().contains("axon:local"),
        "error should name the missing scope; got: {}",
        err.message
    );
}

/// The same local-filesystem source is allowed once the caller also holds
/// `axon:local`.
#[tokio::test]
async fn source_local_path_allowed_with_local_scope() {
    let source_dir = tempfile::tempdir().expect("tempdir");
    let snapshot = snapshot_with_scopes(&["axon:write", "axon:local"]);
    enforce_source_safety_scope(&source_dir.path().to_string_lossy(), Some(&snapshot))
        .await
        .expect("local source with axon:local must pass authorization");
}

/// A web-URL source is unaffected by the local-filesystem scope upgrade — a
/// caller holding only `axon:write` (the broad scope the router-level gate
/// already requires for the `source` action) is still allowed through, same
/// as before this fix.
#[tokio::test]
async fn source_web_url_allowed_with_write_scope_only() {
    let snapshot = snapshot_with_scopes(&["axon:write"]);
    enforce_source_safety_scope("https://example.com", Some(&snapshot))
        .await
        .expect("web source with axon:write must pass authorization");
}

#[tokio::test]
async fn source_missing_input_returns_invalid_params() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let req = SourceRequest::default();

    let result = server.handle_source(req).await;
    let err = result.expect_err("source without input must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.to_lowercase().contains("source")
            || err.message.to_lowercase().contains("input"),
        "error should mention the missing source/input; got: {}",
        err.message
    );
}

#[tokio::test]
async fn source_blank_input_returns_invalid_params() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let req = SourceRequest {
        source: Some("   ".to_string()),
        ..Default::default()
    };

    let result = server.handle_source(req).await;
    let err = result.expect_err("blank source must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

#[test]
fn source_without_data_plane_fails_at_provider_boundary() {
    // Provider construction is lazy (`ServiceContext::build_target_local_source`),
    // so with no qdrant/tei configured an indexing request still routes through
    // `axon_services::index_source` and fails at the provider boundary (fetch or
    // vector provider, depending on network reachability). `handle_source`
    // surfaces that as the service's wrapped source failure — proving the
    // request reached the pipeline instead of being rejected during parsing.
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        qdrant_url: String::new(),
        tei_url: String::new(),
        // Isolate the jobs DB so building the service context does not collide
        // with a shared on-disk jobs.db from another checkout.
        sqlite_path: tmp.path().join("jobs.db"),
        ..axon_core::config::Config::default()
    };
    let server = AxonMcpServer::new(cfg);
    let req = SourceRequest {
        source: Some("https://example.com".to_string()),
        ..Default::default()
    };

    // This deliberately enters the full web acquisition pipeline. Give the
    // Tokio worker/blocking pool explicit stack headroom rather than relying
    // on the test macro's small default worker stack.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(2)
        .thread_stack_size(8 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("build source test runtime");
    let err = runtime
        .block_on(server.handle_source(req))
        .expect_err("indexing without a data plane fails at the provider boundary");
    assert_eq!(err.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
    assert!(
        err.message.contains("source 'https://example.com' failed"),
        "error must be the service-wrapped source failure; got: {}",
        err.message
    );
}
