use super::*;
use crate::ops::ranking::AskCandidate;
use axon_core::config::Config;
use httpmock::prelude::*;
use std::collections::HashSet;

fn candidate(url: &str) -> AskCandidate {
    AskCandidate {
        score: 1.0,
        url: url.to_string(),
        path: "/p".to_string(),
        chunk_text: "chunk".to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score: 1.0,
    }
}

/// One filter-only batch query body per supplied URL, in order.
fn batch_response_for(urls: &[&str]) -> serde_json::Value {
    let result: Vec<serde_json::Value> = urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            serde_json::json!({
                "points": [{
                    "id": format!("{url}:0"),
                    "score": 0.9,
                    "payload": {
                        "url": url,
                        "chunk_text": "body",
                        "chunk_index": i
                    }
                }]
            })
        })
        .collect();
    serde_json::json!({ "result": result })
}

/// N unique URLs (cache disabled) must be fetched with a SINGLE batched Qdrant
/// request — the N+1 guard. If this path regressed to per-URL scroll it would
/// issue 3 requests and this assertion (hits == 1, no scroll calls) would fail.
#[tokio::test]
async fn fetch_full_docs_batches_unique_urls_in_one_request() {
    let server = MockServer::start_async().await;
    let urls = ["https://x.com/a", "https://x.com/b", "https://x.com/c"];
    let batch_mock = server
        .mock_async(|when, then| {
            when.method(POST).path_includes("points/query/batch");
            then.status(200).json_body(batch_response_for(&urls));
        })
        .await;
    // Guard: the per-URL scroll fallback must NOT be exercised on the happy path.
    let scroll_mock = server
        .mock_async(|when, then| {
            when.method(POST).path_includes("points/scroll");
            then.status(200)
                .json_body(serde_json::json!({"result": {"points": []}}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test".to_string();
    cfg.ask_cache_enabled = false;

    let reranked: Vec<AskCandidate> = urls.iter().map(|u| candidate(u)).collect();
    let indices = vec![0usize, 1, 2];
    let result = fetch_full_docs(&cfg, &reranked, &indices, 0, 100_000, 50, 8)
        .await
        .expect("batch fetch must succeed");

    assert_eq!(result.docs.len(), 3, "all three docs must come back");
    assert!(result.errors.is_empty(), "no errors on the happy path");
    // The N+1 guard: exactly one batch request, zero scroll requests.
    assert_eq!(
        batch_mock.calls_async().await,
        1,
        "N unique URLs must be a single batched request"
    );
    assert_eq!(
        scroll_mock.calls_async().await,
        0,
        "happy-path batch must not fall back to per-URL scroll"
    );
}

/// A single URL's empty fetch in the batch must NOT abort the others — that URL
/// is recorded as an error while the remaining docs are still returned (graceful
/// degradation).
#[tokio::test]
async fn fetch_full_docs_one_empty_url_does_not_abort_others() {
    let server = MockServer::start_async().await;
    // Middle URL ("b") comes back with zero points; a and c have content.
    let response = serde_json::json!({
        "result": [
            {"points": [{"id": "a:0", "score": 0.9, "payload": {"url": "https://x.com/a", "chunk_text": "body", "chunk_index": 0}}]},
            {"points": []},
            {"points": [{"id": "c:0", "score": 0.9, "payload": {"url": "https://x.com/c", "chunk_text": "body", "chunk_index": 0}}]}
        ]
    });
    let _mock = server
        .mock_async(|when, then| {
            when.method(POST).path_includes("points/query/batch");
            then.status(200).json_body(response);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test".to_string();
    cfg.ask_cache_enabled = false;

    let reranked = vec![
        candidate("https://x.com/a"),
        candidate("https://x.com/b"),
        candidate("https://x.com/c"),
    ];
    let result = fetch_full_docs(&cfg, &reranked, &[0, 1, 2], 0, 100_000, 50, 8)
        .await
        .expect("fetch must succeed even with one empty URL");

    // a and c survive; b is recorded as an error, not a panic/abort.
    assert_eq!(result.docs.len(), 2, "the two non-empty docs must survive");
    let kept_urls: HashSet<&str> = result.docs.iter().map(|(_, u, _)| u.as_str()).collect();
    assert!(kept_urls.contains("https://x.com/a"));
    assert!(kept_urls.contains("https://x.com/c"));
    assert_eq!(result.errors.len(), 1, "the empty URL must be one error");
    assert_eq!(result.errors[0].url, "https://x.com/b");
}

/// When the batch endpoint fails, the code must fall back to per-URL scroll and
/// a single failing URL there must not abort the others. This exercises the
/// `buffer_unordered` per-URL path's graceful degradation. Each unique URL gets
/// exactly one scroll request (per-URL N+1 surface).
#[tokio::test]
async fn fetch_full_docs_falls_back_to_per_url_scroll_on_batch_error() {
    let server = MockServer::start_async().await;
    // Batch endpoint always 4xx → non-retryable, triggers the scroll fallback.
    let _batch = server
        .mock_async(|when, then| {
            when.method(POST).path_includes("points/query/batch");
            then.status(400).body("bad request");
        })
        .await;
    // Per-URL scroll mocks, matched by the URL filter embedded in the scroll body.
    // /a and /c return one point each; /b returns an empty page (its error must
    // not abort the others).
    let scroll_doc = |url: &str| {
        serde_json::json!({
            "result": {
                "points": [{
                    "id": format!("{url}:0"),
                    "payload": {"url": url, "chunk_text": "body", "chunk_index": 0}
                }],
                "next_page_offset": null
            }
        })
    };
    let mock_a = server
        .mock_async(|when, then| {
            when.method(POST)
                .path_includes("points/scroll")
                .body_includes("https://x.com/a");
            then.status(200).json_body(scroll_doc("https://x.com/a"));
        })
        .await;
    // /b hard-fails (non-retryable 400). Its failure must not abort /a and /c.
    let mock_b = server
        .mock_async(|when, then| {
            when.method(POST)
                .path_includes("points/scroll")
                .body_includes("https://x.com/b");
            then.status(400).body("bad request");
        })
        .await;
    let mock_c = server
        .mock_async(|when, then| {
            when.method(POST)
                .path_includes("points/scroll")
                .body_includes("https://x.com/c");
            then.status(200).json_body(scroll_doc("https://x.com/c"));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "test".to_string();
    cfg.ask_cache_enabled = false;

    let reranked = vec![
        candidate("https://x.com/a"),
        candidate("https://x.com/b"),
        candidate("https://x.com/c"),
    ];
    let result = fetch_full_docs(&cfg, &reranked, &[0, 1, 2], 0, 100_000, 50, 8)
        .await
        .expect("scroll fallback must succeed");

    assert_eq!(
        result.docs.len(),
        2,
        "a and c must survive the per-URL fallback"
    );
    let kept_urls: HashSet<&str> = result.docs.iter().map(|(_, u, _)| u.as_str()).collect();
    assert!(kept_urls.contains("https://x.com/a"));
    assert!(kept_urls.contains("https://x.com/c"));
    // /b's failure is recorded, not propagated as an abort.
    assert_eq!(result.errors.len(), 1, "the failing URL must be one error");
    assert_eq!(result.errors[0].url, "https://x.com/b");
    // One scroll request per unique URL — the per-URL N+1 surface.
    assert_eq!(mock_a.calls_async().await, 1, "one scroll for /a");
    assert_eq!(mock_b.calls_async().await, 1, "one scroll for /b");
    assert_eq!(mock_c.calls_async().await, 1, "one scroll for /c");
}
