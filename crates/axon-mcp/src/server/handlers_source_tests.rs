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
/// `index_source_with_auth`. Provider construction is lazy (`ServiceContext::
/// build_target_local_source`), so with no qdrant/tei configured the request
/// reaches the provider boundary and fails there; the assertion is only that
/// the scope gate did not refuse it.
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

    assert_passed_source_authorization(result);
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

    assert_passed_source_authorization(result);
}

/// Assert a `handle_source` outcome got past `enforce_source_safety_scope`:
/// either the pipeline succeeded, or it failed later (e.g. at the lazily
/// constructed provider boundary when no data plane is configured) with an
/// error that is not the scope refusal.
fn assert_passed_source_authorization(result: Result<AxonToolResponse, ErrorData>) {
    if let Err(err) = result {
        assert_ne!(
            err.code,
            rmcp::model::ErrorCode::INVALID_REQUEST,
            "request must not be refused at the authorization boundary; got: {}",
            err.message
        );
        assert!(
            !err.message.contains("axon:local"),
            "error must not be the axon:local scope refusal; got: {}",
            err.message
        );
    }
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
async fn source_without_data_plane_fails_at_provider_boundary() {
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

    let err = server
        .handle_source(req)
        .await
        .expect_err("indexing without a data plane fails at the provider boundary");
    assert_eq!(err.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
    assert!(
        err.message.contains("source 'https://example.com' failed"),
        "error must be the service-wrapped source failure; got: {}",
        err.message
    );
}
