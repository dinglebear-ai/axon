use super::*;
use crate::core::config::Config;
use httpmock::prelude::*;

fn make_cfg(base_url: String) -> Config {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = base_url;
    cfg.collection = "test_col".to_string();
    cfg
}

fn ok_body() -> serde_json::Value {
    serde_json::json!({"result": true, "status": "ok", "time": 0.001})
}

#[tokio::test]
async fn ensure_payload_indexes_fires_one_put_per_field() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/test_col/index");
            then.status(200).json_body(ok_body());
        })
        .await;

    let cfg = make_cfg(server.base_url());
    ensure_payload_indexes(&cfg).await.expect("should succeed");

    // keyword(N) + integer(8) + datetime(1) + bool(2) = KEYWORD_INDEX_FIELDS.len() + 11
    assert_eq!(
        mock.calls_async().await,
        KEYWORD_INDEX_FIELDS.len() + 11,
        "expected exactly one PUT per indexed field"
    );
}

#[tokio::test]
async fn ensure_payload_indexes_fails_when_qdrant_always_errors() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/test_col/index");
            then.status(503)
                .json_body(serde_json::json!({"status": "error"}));
        })
        .await;

    let cfg = make_cfg(server.base_url());
    let result = ensure_payload_indexes(&cfg).await;
    assert!(result.is_err(), "should propagate Qdrant errors");
}

#[tokio::test]
async fn put_index_with_retry_succeeds_on_200() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/index");
            then.status(200).json_body(ok_body());
        })
        .await;

    let client = internal_service_http_client().unwrap();
    let url = format!("{}/index", server.base_url());
    let result = put_index_with_retry(
        client.clone(),
        url,
        serde_json::json!({"field_name": "url", "field_schema": "keyword"}),
    )
    .await;

    assert!(result.is_ok());
    // Exactly one request — no unnecessary retries on success.
    assert_eq!(mock.calls_async().await, 1);
}

#[tokio::test]
async fn put_index_with_retry_exhausts_all_attempts_on_persistent_error() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/index");
            then.status(503)
                .json_body(serde_json::json!({"status": "error"}));
        })
        .await;

    let client = internal_service_http_client().unwrap();
    let url = format!("{}/index", server.base_url());
    let result = put_index_with_retry(
        client.clone(),
        url,
        serde_json::json!({"field_name": "url", "field_schema": "keyword"}),
    )
    .await;

    assert!(result.is_err(), "should fail after exhausting retries");
    assert_eq!(
        mock.calls_async().await,
        MAX_INDEX_ATTEMPTS as usize,
        "should attempt exactly MAX_INDEX_ATTEMPTS times"
    );
}
