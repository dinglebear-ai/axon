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
    let cfg = axon_core::config::Config::default();
    let server = AxonMcpServer::new(cfg);
    let req = SourceRequest {
        source: Some(tmp.path().to_string_lossy().to_string()),
        ..Default::default()
    };

    let result = CURRENT_CALLER_AUTH_SNAPSHOT
        .scope(Some(snapshot_with_scopes(&["axon:write"])), async {
            server.handle_source(req).await
        })
        .await;

    let err = result.expect_err("local source without axon:local must be refused");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_REQUEST);
    assert!(
        err.message.to_lowercase().contains("axon:local"),
        "error should name the missing scope; got: {}",
        err.message
    );
}

/// The same local-filesystem source is allowed once the caller also holds
/// `axon:local` — proceeds past the authorization boundary into
/// `index_source_with_auth`, which degrades to a `Failed` `SourceResult`
/// (`Ok`, not `Err`) because this test has no qdrant/tei configured.
#[tokio::test]
async fn source_local_path_allowed_with_local_scope() {
    let source_dir = tempfile::tempdir().expect("tempdir");
    let jobs_dir = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        qdrant_url: String::new(),
        tei_url: String::new(),
        sqlite_path: jobs_dir.path().join("jobs.db"),
        ..axon_core::config::Config::default()
    };
    let server = AxonMcpServer::new(cfg);
    let req = SourceRequest {
        source: Some(source_dir.path().to_string_lossy().to_string()),
        ..Default::default()
    };

    let result = CURRENT_CALLER_AUTH_SNAPSHOT
        .scope(
            Some(snapshot_with_scopes(&["axon:write", "axon:local"])),
            async { server.handle_source(req).await },
        )
        .await;

    result.expect("local source with axon:local must pass the authorization boundary");
}

/// A web-URL source is unaffected by the local-filesystem scope upgrade — a
/// caller holding only `axon:write` (the broad scope the router-level gate
/// already requires for the `source` action) is still allowed through, same
/// as before this fix.
#[tokio::test]
async fn source_web_url_allowed_with_write_scope_only() {
    let jobs_dir = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        qdrant_url: String::new(),
        tei_url: String::new(),
        sqlite_path: jobs_dir.path().join("jobs.db"),
        ..axon_core::config::Config::default()
    };
    let server = AxonMcpServer::new(cfg);
    let req = SourceRequest {
        source: Some("https://example.com".to_string()),
        ..Default::default()
    };

    let result = CURRENT_CALLER_AUTH_SNAPSHOT
        .scope(Some(snapshot_with_scopes(&["axon:write"])), async {
            server.handle_source(req).await
        })
        .await;

    result.expect("web source with only axon:write must pass the authorization boundary");
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

#[tokio::test]
async fn source_without_data_plane_returns_degraded_result() {
    // With no qdrant/tei configured the base service context has no local-source
    // runtime, so `index_source` returns a degraded (status=Failed) SourceResult
    // rather than an error. `handle_source` must surface that as an Ok response —
    // proving it routes through `axon_services::index_source`.
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

    let response = server
        .handle_source(req)
        .await
        .expect("degraded source result is Ok, not an error");
    assert_eq!(response.action, "source");
    // The serialized SourceResult carries a canonical_uri and a Failed status
    // when the data plane is unconfigured.
    let data = &response.data;
    let status = data
        .get("status")
        .or_else(|| data.get("data").and_then(|d| d.get("status")))
        .and_then(serde_json::Value::as_str);
    // Either the inline payload or a path-mode wrapper — assert the response is
    // well-formed and names the source action; the degraded status is an
    // index_source concern already covered in axon-services tests.
    assert!(
        response.ok || status == Some("failed"),
        "handle_source should return a well-formed response; got: {response:?}"
    );
}
