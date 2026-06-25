use super::{BATCH_RETRIEVE_URL_CAP, parse_retrieve_scroll_points, retrieve_scroll_limit};
use axon_core::config::Config;
use httpmock::prelude::*;

#[test]
fn retrieve_scroll_limit_honors_small_max_points() {
    assert_eq!(retrieve_scroll_limit(Some(1)), 1);
    assert_eq!(retrieve_scroll_limit(Some(42)), 42);
    assert_eq!(retrieve_scroll_limit(Some(0)), 1);
    assert_eq!(retrieve_scroll_limit(None), 256);
    assert_eq!(retrieve_scroll_limit(Some(500)), 256);
}

#[test]
fn parse_retrieve_scroll_points_counts_malformed_points() {
    let points = vec![
        serde_json::json!({
            "id": "ok",
            "payload": {
                "url": "https://example.com",
                "chunk_text": "hello",
                "chunk_index": 0
            }
        }),
        serde_json::json!({
            "id": "bad",
            "payload": {
                "url": 123,
                "chunk_text": "bad"
            }
        }),
    ];
    let (parsed, malformed) = parse_retrieve_scroll_points(&points);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].payload.url, "https://example.com");
    assert_eq!(malformed, 1);
}

#[tokio::test]
async fn batch_retrieve_empty_urls_returns_ok_without_network() {
    let cfg = Config::test_default();
    let result = super::qdrant_batch_retrieve_by_urls(&cfg, &[], None).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn batch_retrieve_over_cap_returns_err() {
    let cfg = Config::test_default();
    let urls: Vec<String> = (0..=BATCH_RETRIEVE_URL_CAP)
        .map(|i| format!("https://example.com/{i}"))
        .collect();
    let result = super::qdrant_batch_retrieve_by_urls(&cfg, &urls, None).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("batch too large"), "unexpected error: {msg}");
}

#[tokio::test]
async fn batch_retrieve_result_count_mismatch_returns_err() {
    let server = MockServer::start_async().await;
    let _mock = server
        .mock_async(|when, then| {
            when.method(POST).path_includes("points/query/batch");
            then.status(200).json_body(serde_json::json!({
                "result": []
            }));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test".to_string();

    let urls = vec!["https://example.com/a".to_string()];
    let result = super::qdrant_batch_retrieve_by_urls(&cfg, &urls, None).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("expected 1 result sets, got 0"),
        "unexpected error: {msg}"
    );
}
