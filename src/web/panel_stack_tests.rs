use super::{
    StackResponse, StackRuntimeMode, browser_display_host, host_prerequisite_checks,
    qwen3_model_reported, tei_check,
};
use httpmock::Method::GET;
use httpmock::MockServer;
use serde_json::json;
use std::path::Path;

#[test]
fn display_host_normalizes_wildcard_binds_for_browser_urls() {
    assert_eq!(browser_display_host("0.0.0.0"), "127.0.0.1");
    assert_eq!(browser_display_host("::"), "127.0.0.1");
    assert_eq!(browser_display_host("[::]"), "127.0.0.1");
    assert_eq!(browser_display_host("192.0.2.10"), "192.0.2.10");
}

#[test]
fn stack_response_json_shape_includes_runtime_and_checks() {
    let response = StackResponse {
        runtime_mode: "host",
        server_url: "http://127.0.0.1:8001".to_string(),
        mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
        log_dir: "/tmp/axon/logs".to_string(),
        compose_file: "/tmp/axon/compose/docker-compose.yaml".to_string(),
        checks: vec![super::check("Qdrant", "ok", "ready")],
    };

    let value = serde_json::to_value(response).unwrap();
    assert_eq!(value["runtime_mode"], "host");
    assert_eq!(value["server_url"], "http://127.0.0.1:8001");
    assert_eq!(value["mcp_url"], "http://127.0.0.1:8001/mcp");
    assert_eq!(
        value["checks"][0],
        json!({
            "label": "Qdrant",
            "status": "ok",
            "detail": "ready",
        })
    );
}

#[tokio::test]
async fn container_mode_skips_host_prerequisite_failures() {
    let checks = host_prerequisite_checks(StackRuntimeMode::Container, Path::new("/missing")).await;

    let labels: Vec<_> = checks.iter().map(|check| check.label).collect();
    assert_eq!(
        labels,
        vec![
            "Docker",
            "Docker Compose",
            "NVIDIA runtime",
            "Compose assets",
            "Gemini CLI",
        ]
    );
    assert!(checks.iter().all(|check| check.status == "skipped"));
    assert!(
        checks
            .iter()
            .all(|check| check.detail.contains("container-served panel"))
    );
}

#[test]
fn qwen3_model_detection_accepts_qwen3_variants() {
    assert!(qwen3_model_reported(
        r#"{"model_id":"Qwen/Qwen3-Embedding-0.6B"}"#
    ));
    assert!(qwen3_model_reported("text-embeddings-qwen3"));
    assert!(!qwen3_model_reported(r#"{"model_id":"BAAI/bge-large-en"}"#));
}

#[tokio::test]
async fn tei_check_requires_info_qwen3_after_health() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/health");
            then.status(200).body("ok");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(200).json_body(json!({
                "model_id": "Qwen/Qwen3-Embedding-0.6B"
            }));
        })
        .await;

    let check = tei_check(&server.base_url()).await;
    assert_eq!(check.status, "ok");
    assert!(check.detail.contains("Qwen3 model reported"));
}

#[tokio::test]
async fn tei_check_warns_when_info_lacks_qwen3() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/health");
            then.status(200).body("ok");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(200).json_body(json!({
                "model_id": "BAAI/bge-large-en"
            }));
        })
        .await;

    let check = tei_check(&server.base_url()).await;
    assert_eq!(check.status, "warn");
    assert!(check.detail.contains("/info"));
    assert!(check.detail.contains("Qwen3"));
}

#[tokio::test]
async fn tei_check_errors_when_info_is_unavailable() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/health");
            then.status(200).body("ok");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(503).body("warming");
        })
        .await;

    let check = tei_check(&server.base_url()).await;
    assert_eq!(check.status, "error");
    assert!(check.detail.contains("/info"));
    assert!(check.detail.contains("503"));
}
