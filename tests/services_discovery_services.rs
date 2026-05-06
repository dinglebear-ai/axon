use axon::crates::services::map::parse_map_result;
use axon::crates::services::scrape::map_scrape_payload;
use axon::crates::services::search::{map_research_payload, map_search_results};

// ---------------------------------------------------------------------------
// scrape service — map_scrape_payload
// ---------------------------------------------------------------------------

#[test]
fn maps_scrape_payload_to_scrape_result() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "status_code": 200,
        "markdown": "# Hello\n\nWorld",
        "title": "Hello",
        "description": "A page",
    });
    let result = map_scrape_payload(payload.clone()).expect("valid scrape payload");
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_scrape_payload_preserves_arbitrary_fields() {
    let payload = serde_json::json!({
        "url": "https://example.com/docs",
        "status_code": 200,
        "markdown": "content",
        "title": "Docs",
        "description": "",
        "extra_field": "should be preserved",
    });
    let result = map_scrape_payload(payload.clone()).expect("valid scrape payload");
    assert_eq!(result.payload["extra_field"], "should be preserved");
}

#[test]
fn maps_scrape_payload_with_non_200_status() {
    let payload = serde_json::json!({
        "url": "https://example.com/missing",
        "status_code": 404,
        "markdown": "",
        "title": "Not Found",
        "description": "",
    });
    let result =
        map_scrape_payload(payload.clone()).expect("non-200 scrape payload is still mappable");
    assert_eq!(result.payload["status_code"], 404);
}

#[test]
fn maps_scrape_payload_requires_required_fields() {
    let payload = serde_json::json!({"a": 1, "b": [1, 2, 3]});
    let err = map_scrape_payload(payload).expect_err("payload missing required fields should fail");
    assert!(err.to_string().contains("scrape payload missing url"));
}

// ---------------------------------------------------------------------------
// map service — MapResult (typed, via parse_map_result)
// ---------------------------------------------------------------------------

#[test]
fn maps_map_payload_to_map_result() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "mapped_urls": 3u64,
        "total": 3u64,
        "sitemap_urls": 2usize,
        "pages_seen": 3u32,
        "thin_pages": 0u32,
        "elapsed_ms": 450u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": ["https://example.com/a", "https://example.com/b", "https://example.com/c"],
    });
    let result = parse_map_result(payload).expect("valid map payload");
    assert_eq!(result.url, "https://example.com");
    assert_eq!(result.returned_url_count, 3);
    assert_eq!(result.sitemap_urls, 2);
    assert_eq!(result.pages_seen, 3);
    assert_eq!(result.urls.len(), 3);
}

#[test]
fn maps_map_payload_preserves_urls_array() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "mapped_urls": 2u64,
        "total": 2u64,
        "sitemap_urls": 0usize,
        "pages_seen": 2u32,
        "thin_pages": 0u32,
        "elapsed_ms": 200u64,
        "map_source": "crawl",
        "warning": null,
        "urls": ["https://example.com/one", "https://example.com/two"],
    });
    let result = parse_map_result(payload).expect("valid map payload");
    assert_eq!(result.urls.len(), 2);
    assert_eq!(result.urls[0], "https://example.com/one");
    assert_eq!(result.urls[1], "https://example.com/two");
}

#[test]
fn maps_map_payload_with_empty_urls() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 50u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": [],
    });
    let result = parse_map_result(payload).expect("valid map payload");
    assert_eq!(result.returned_url_count, 0);
    assert!(result.urls.is_empty());
}

#[test]
fn maps_map_payload_rejects_missing_required_field() {
    // A payload missing `map_source` must fail to parse.
    let payload = serde_json::json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "warning": null,
        "urls": []
        // map_source intentionally omitted
    });
    let err = parse_map_result(payload).unwrap_err();
    assert!(
        err.to_string().contains("map_source") || err.to_string().contains("missing field"),
        "expected error about missing field, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// search service — map_search_results
// ---------------------------------------------------------------------------

#[test]
fn maps_search_results_to_search_result() {
    let results = vec![
        serde_json::json!({"position": 1, "title": "Result One", "url": "https://a.com", "snippet": "snippet one"}),
        serde_json::json!({"position": 2, "title": "Result Two", "url": "https://b.com", "snippet": "snippet two"}),
    ];
    let result = map_search_results(results.clone());
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0]["title"], "Result One");
    assert_eq!(result.results[1]["url"], "https://b.com");
}

#[test]
fn maps_empty_search_results() {
    let results: Vec<serde_json::Value> = vec![];
    let result = map_search_results(results);
    assert!(result.results.is_empty());
}

#[test]
fn maps_search_result_preserves_all_fields() {
    let item = serde_json::json!({
        "position": 1,
        "title": "Test",
        "url": "https://test.com",
        "snippet": "A snippet",
    });
    let result = map_search_results(vec![item.clone()]);
    assert_eq!(result.results[0], item);
}

#[test]
fn maps_single_search_result() {
    let results = vec![
        serde_json::json!({"position": 1, "title": "Solo", "url": "https://solo.com", "snippet": null}),
    ];
    let result = map_search_results(results);
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0]["title"], "Solo");
}

// ---------------------------------------------------------------------------
// research service — map_research_payload
// ---------------------------------------------------------------------------

#[test]
fn maps_research_payload_to_research_result() {
    let payload = serde_json::json!({
        "query": "rust async patterns",
        "limit": 5,
        "offset": 0,
        "search_results": [],
        "extractions": [],
        "summary": "A comprehensive summary.",
        "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150},
        "timing_ms": {"total": 1200},
    });
    let result = map_research_payload(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_research_payload_preserves_summary() {
    let payload = serde_json::json!({
        "query": "test",
        "summary": "The summary goes here.",
    });
    let result = map_research_payload(payload.clone());
    assert_eq!(result.payload["summary"], "The summary goes here.");
}

#[test]
fn maps_research_payload_with_null_summary() {
    let payload = serde_json::json!({
        "query": "test",
        "summary": null,
    });
    let result = map_research_payload(payload.clone());
    assert!(result.payload["summary"].is_null());
}

#[test]
fn maps_research_payload_wraps_json_value_verbatim() {
    let payload = serde_json::json!({"anything": [1, 2, 3]});
    let result = map_research_payload(payload.clone());
    assert_eq!(result.payload, payload);
}
