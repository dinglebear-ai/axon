use super::*;
use crate::schema::ResolveRequest;

#[tokio::test]
async fn resolve_missing_source_returns_invalid_params() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let req = ResolveRequest {
        source: None,
        response_mode: None,
    };

    let result = server.handle_resolve(req).await;
    let err = result.expect_err("resolve without source must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

#[tokio::test]
async fn resolve_blank_source_returns_invalid_params() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let req = ResolveRequest {
        source: Some("   ".to_string()),
        response_mode: None,
    };

    let result = server.handle_resolve(req).await;
    let err = result.expect_err("blank source must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

// Note: a success-path test (valid web URL -> routed kind) is intentionally
// not included here. `respond_with_mode` always writes a provenance artifact
// regardless of response_mode (see `artifacts/respond.rs`), and artifact-root
// sandboxing is environment/cwd-sensitive in the test harness (shared with
// every other handler's respond_with_mode call, not specific to resolve) —
// see `server::handlers_source::tests::source_without_data_plane_returns_degraded_result`
// for the same pattern exercised elsewhere. The validation-only tests above
// cover `handle_resolve`'s own logic without touching that shared plumbing.

#[test]
fn provider_summaries_reshapes_doctor_payload() {
    let payload = serde_json::json!({
        "services": {
            "qdrant": { "ok": true, "latency_ms": 5 },
            "tei": { "ok": false, "error": "timeout" }
        }
    });
    let providers = provider_summaries(&payload);
    assert_eq!(providers.len(), 2);
    // Sorted by id.
    assert_eq!(providers[0]["id"], "qdrant");
    assert_eq!(providers[0]["ok"], true);
    assert_eq!(providers[1]["id"], "tei");
    assert_eq!(providers[1]["ok"], false);
}

#[test]
fn provider_summaries_empty_when_services_missing() {
    let payload = serde_json::json!({});
    assert!(provider_summaries(&payload).is_empty());
}
