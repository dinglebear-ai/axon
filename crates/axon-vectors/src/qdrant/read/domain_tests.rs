use super::*;
use serde_json::json;

#[test]
fn domain_chunk0_filter_shape() {
    let filter = domain_chunk0_filter("example.com");
    assert_eq!(
        filter,
        json!({
            "must": [
                {"key": "web_domain", "match": {"value": "example.com"}},
                {"key": "chunk_index", "match": {"value": 0}}
            ]
        })
    );
}

#[test]
fn canonical_uri_from_payload_prefers_target_item_uri() {
    let payload = json!({
        "item_canonical_uri": "https://example.com/docs/page",
        "source_canonical_uri": "https://example.com/docs",
        "source_item_key": "page",
        "chunk_locator": { "canonical_uri": "https://example.com/docs/page#chunk" }
    });

    assert_eq!(
        canonical_uri_from_payload(&payload),
        Some("https://example.com/docs/page")
    );
}

#[test]
fn canonical_uri_from_payload_falls_back_to_chunk_locator() {
    let payload = json!({
        "chunk_locator": { "canonical_uri": "https://example.com/docs/page#chunk" }
    });

    assert_eq!(
        canonical_uri_from_payload(&payload),
        Some("https://example.com/docs/page#chunk")
    );
}

#[test]
fn cursor_round_trips_string_offset() {
    let encoded = encode_scroll_cursor(json!("point-123"));
    assert_eq!(encoded, "point-123");
    let decoded = decode_scroll_cursor(&encoded);
    assert_eq!(decoded, json!("point-123"));
}

#[test]
fn cursor_round_trips_non_string_offset() {
    let encoded = encode_scroll_cursor(json!({"start_from": 7}));
    let decoded = decode_scroll_cursor(&encoded);
    assert_eq!(decoded, json!({"start_from": 7}));
}

#[test]
fn cursor_decode_falls_back_to_bare_string_for_invalid_json() {
    let decoded = decode_scroll_cursor("not-json-{{{");
    assert_eq!(decoded, json!("not-json-{{{"));
}
