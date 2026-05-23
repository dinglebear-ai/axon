use super::*;
use crate::core::config::Config;
use serde_json::json;

#[test]
fn map_sources_valid() {
    let payload = json!({
        "count": 2,
        "limit": 10,
        "offset": 0,
        "urls": [
            { "url": "https://example.com/a", "chunks": 3 },
            { "url": "https://example.com/b", "chunks": 7 }
        ]
    });
    let result = map_sources_payload(&payload).unwrap();
    assert_eq!(result.count, 2);
    assert_eq!(result.limit, 10);
    assert_eq!(result.offset, 0);
    assert_eq!(result.urls.len(), 2);
    assert_eq!(result.urls[0], ("https://example.com/a".to_string(), 3));
    assert_eq!(result.urls[1], ("https://example.com/b".to_string(), 7));
}

#[test]
fn map_sources_missing_count() {
    let payload = json!({ "limit": 10, "offset": 0, "urls": [] });
    let err = map_sources_payload(&payload).unwrap_err();
    assert!(
        err.to_string().contains("count"),
        "error must mention 'count', got: {err}"
    );
}

#[test]
fn map_sources_missing_urls() {
    let payload = json!({ "count": 0, "limit": 10, "offset": 0 });
    let err = map_sources_payload(&payload).unwrap_err();
    assert!(
        err.to_string().contains("urls"),
        "error must mention 'urls', got: {err}"
    );
}

#[test]
fn map_sources_url_entry_missing_url_field() {
    let payload = json!({
        "count": 1,
        "limit": 10,
        "offset": 0,
        "urls": [{ "chunks": 5 }]
    });
    let err = map_sources_payload(&payload).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("urls[0]"),
        "error must reference urls[0], got: {msg}"
    );
}

#[test]
fn map_sources_url_entry_missing_chunks_field() {
    let payload = json!({
        "count": 1,
        "limit": 10,
        "offset": 0,
        "urls": [{ "url": "https://example.com" }]
    });
    let err = map_sources_payload(&payload).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("chunks"),
        "error must mention 'chunks', got: {msg}"
    );
}

#[test]
fn map_sources_empty_urls_array() {
    let payload = json!({ "count": 0, "limit": 50, "offset": 0, "urls": [] });
    let result = map_sources_payload(&payload).unwrap();
    assert_eq!(result.count, 0);
    assert!(result.urls.is_empty());
}

#[test]
fn normalize_domain_query_accepts_host_url_case_and_trailing_dot() {
    assert_eq!(normalize_domain_query(" Docs.RS. ").unwrap(), "docs.rs");
    assert_eq!(normalize_domain_query("docs.rs:443").unwrap(), "docs.rs");
    assert_eq!(
        normalize_domain_query("https://Docs.RS/std/index.html").unwrap(),
        "docs.rs"
    );
}

#[test]
fn normalize_domain_query_rejects_empty_unknown_and_control_chars() {
    assert!(normalize_domain_query(" ").is_err());
    assert!(normalize_domain_query("unknown").is_err());
    assert!(normalize_domain_query("*.docs.rs").is_err());
    assert!(normalize_domain_query("docs.rs\nbad").is_err());
}

#[test]
fn domain_sources_from_urls_preserves_cursor_page() {
    let result = domain_sources_from_urls(
        "docs.rs".to_string(),
        vec![
            "https://docs.rs/z".to_string(),
            "https://docs.rs/a".to_string(),
        ],
        2,
        Some("cursor-a".to_string()),
        Some("cursor-b".to_string()),
    );

    assert_eq!(result.count, 2);
    assert_eq!(result.urls, vec!["https://docs.rs/z", "https://docs.rs/a"]);
    assert!(result.truncated);
    assert_eq!(result.cursor.as_deref(), Some("cursor-a"));
    assert_eq!(result.next_cursor.as_deref(), Some("cursor-b"));
}

#[test]
fn domain_sources_from_urls_reports_terminal_page() {
    let result = domain_sources_from_urls(
        "docs.rs".to_string(),
        vec![
            "https://docs.rs/a".to_string(),
            "https://docs.rs/b".to_string(),
        ],
        2,
        None,
        None,
    );

    assert_eq!(result.urls, vec!["https://docs.rs/a", "https://docs.rs/b"]);
    assert!(!result.truncated);
    assert_eq!(result.next_cursor, None);
}

#[tokio::test]
async fn sources_for_domain_rejects_offset_pagination_before_qdrant() {
    let cfg = Config::test_default();
    let err = sources_for_domain(
        &cfg,
        "docs.rs",
        Pagination {
            limit: 10,
            offset: 1,
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("cursor"),
        "error should point callers at cursor pagination, got: {err}"
    );
}
