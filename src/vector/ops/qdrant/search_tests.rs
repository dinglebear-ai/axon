use super::*;
use crate::core::config::Config;
use httpmock::HttpMockResponse;
use httpmock::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
    let result: Vec<serde_json::Value> = hits
        .iter()
        .map(|(url, score)| {
            serde_json::json!({
                "id": "test-id",
                "score": score,
                "payload": {"url": url, "chunk_text": "test chunk"}
            })
        })
        .collect();
    serde_json::json!({"result": result})
}

#[tokio::test]
async fn qdrant_search_sends_hnsw_ef_param() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search")
                .json_body_includes(r#"{"params":{"hnsw_ef":64}}"#);
            then.status(200).json_body(make_search_response(vec![(
                "https://example.com/legacy",
                0.85,
            )]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "qdrant_search must succeed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn qdrant_search_sends_quantization_rescore_param() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search")
                .json_body_includes(r#"{"params":{"quantization":{"rescore":true}}}"#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/x", 0.77)]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "rescore param test must succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn qdrant_search_propagates_filter_when_some() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search")
                .json_body_includes(r#"{"filter":{"must":[{"key":"domain"}]}}"#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/f", 0.80)]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let filter = serde_json::json!({
        "must": [{"key": "domain", "match": {"value": "example.com"}}]
    });
    let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, Some(&filter)).await;

    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "filter propagation must succeed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn qdrant_search_recovers_after_retryable_500() {
    let server = MockServer::start_async().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_mock = Arc::clone(&attempts);
    let success_body =
        make_search_response(vec![("https://example.com/retried", 0.81)]).to_string();
    let mock = server
        .mock_async(move |when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search");
            then.respond_with(move |_| {
                if attempts_for_mock.fetch_add(1, Ordering::SeqCst) == 0 {
                    return HttpMockResponse::builder()
                        .status(500)
                        .body("internal error")
                        .build();
                }
                HttpMockResponse::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(success_body.clone())
                    .build()
            });
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let result = qdrant_search(&cfg, &[0.1f32], 5, None).await;
    mock.assert_calls_async(2).await;
    assert!(
        result.is_ok(),
        "retryable HTTP 500 should recover: {:?}",
        result.err()
    );
    assert_eq!(
        result.unwrap()[0].payload.url,
        "https://example.com/retried"
    );
}

#[tokio::test]
async fn qdrant_search_fails_fast_on_non_retryable_400() {
    let server = MockServer::start_async().await;
    let bad_request = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search");
            then.status(400).body("bad request");
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let result = qdrant_search(&cfg, &[0.1f32], 5, None).await;
    bad_request.assert_calls_async(1).await;
    assert!(result.is_err(), "HTTP 400 must fail without retry");
}

#[tokio::test]
async fn qdrant_search_sends_oversampling_param() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/collections/test_col/points/search")
                .json_body_includes(r#"{"params":{"quantization":{"oversampling":1.5}}}"#);
            then.status(200)
                .json_body(make_search_response(vec![("https://example.com/x", 0.77)]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test_col".to_string();

    let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "oversampling param test must succeed: {:?}",
        result.err()
    );
}
