use super::parse_map_result;
use serde_json::json;

// ── parse_map_result ──────────────────────────────────────────────────────

#[test]
fn parse_map_result_valid_full() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 3u64,
        "total": 3u64,
        "sitemap_urls": 3usize,
        "pages_seen": 2u32,
        "thin_pages": 0u32,
        "elapsed_ms": 100u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": [
            "https://example.com/a",
            "https://example.com/b",
            "https://example.com/c"
        ]
    });
    let result = parse_map_result(v).unwrap();
    assert_eq!(result.url, "https://example.com");
    assert_eq!(result.returned_url_count, 3);
    assert_eq!(result.sitemap_urls, 3);
    assert_eq!(result.pages_seen, 2);
    assert_eq!(result.thin_pages, 0);
    assert_eq!(result.elapsed_ms, 100);
    assert_eq!(result.map_source, "sitemap");
    assert!(result.warning.is_none());
    assert_eq!(result.urls.len(), 3);
    assert_eq!(result.urls[0], "https://example.com/a");
}

#[test]
fn parse_map_result_with_warning() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 1u64,
        "total": 1u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 50u64,
        "map_source": "bounded-structure",
        "warning": "too few urls found",
        "urls": ["https://example.com/"]
    });
    let result = parse_map_result(v).unwrap();
    assert_eq!(result.warning.as_deref(), Some("too few urls found"));
    assert_eq!(result.map_source, "bounded-structure");
}

#[test]
fn parse_map_result_missing_url() {
    let v = json!({
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": []
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("url") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_missing_mapped_urls() {
    let v = json!({
        "url": "https://example.com",
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": []
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("mapped_urls") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_missing_urls_array() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("urls") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_empty_urls_array() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "crawl",
        "warning": null,
        "urls": []
    });
    let result = parse_map_result(v).unwrap();
    assert!(result.urls.is_empty());
    assert_eq!(result.returned_url_count, 0);
}

#[test]
fn parse_map_result_round_trips_via_serde() {
    let original = crate::services::types::MapResult {
        url: "https://example.com".to_string(),
        returned_url_count: 2,
        total: 10,
        sitemap_urls: 5,
        pages_seen: 1,
        thin_pages: 0,
        elapsed_ms: 300,
        map_source: "crawl".to_string(),
        warning: Some("low coverage".to_string()),
        urls: vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ],
    };
    let v = serde_json::to_value(&original).unwrap();
    let parsed = parse_map_result(v).unwrap();
    assert_eq!(original, parsed);
}
