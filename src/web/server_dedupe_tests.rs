use super::test_support::{EnvGuard, spawn_full_test_server, stop};
use crate::mcp::auth::AuthPolicy;
use axum::http::StatusCode;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn dedupe_rejects_invalid_collection_before_work() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/dedupe"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "collection": "invalid/name" }))
        .send()
        .await
        .expect("v1 dedupe request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json error");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "bad_request");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("collection"),
        "unexpected body: {body}"
    );
}

#[tokio::test]
#[serial]
async fn dedupe_rejects_body_without_json_content_type() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/dedupe"))
        .header("authorization", "Bearer secret")
        .body(r#"{"collection":"invalid/name"}"#)
        .send()
        .await
        .expect("v1 dedupe request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json error");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(body["kind"], "unsupported_media_type");
}
