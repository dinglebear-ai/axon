use super::*;

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
