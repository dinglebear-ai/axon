use super::{parse_retrieve_scroll_points, retrieve_scroll_limit};

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
