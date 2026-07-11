use super::*;
use serde_json::json;

#[test]
fn domain_chunk0_filter_shape() {
    let filter = domain_chunk0_filter("example.com");
    assert_eq!(
        filter,
        json!({
            "must": [
                {"key": "domain", "match": {"value": "example.com"}},
                {"key": "chunk_index", "match": {"value": 0}}
            ]
        })
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
