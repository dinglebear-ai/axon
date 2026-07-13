use super::*;
use serde_json::json;

#[test]
fn scroll_response_parses_points_and_next_offset() {
    let body = json!({
        "result": {
            "points": [
                {"id": "a-1", "payload": {"url": "https://x/1"}},
                {"id": 42, "payload": {"url": "https://x/2"}}
            ],
            "next_page_offset": "a-2"
        }
    });
    let parsed: ScrollResponse = serde_json::from_value(body).expect("valid scroll response");
    assert_eq!(parsed.result.points.len(), 2);
    assert_eq!(parsed.result.points[0].id, json!("a-1"));
    assert_eq!(
        parsed.result.points[0].payload,
        json!({"url": "https://x/1"})
    );
    assert_eq!(parsed.result.next_page_offset, Some(json!("a-2")));
}

#[test]
fn scroll_response_null_next_offset_is_none() {
    let body = json!({
        "result": { "points": [], "next_page_offset": null }
    });
    let parsed: ScrollResponse = serde_json::from_value(body).expect("valid scroll response");
    assert!(parsed.result.points.is_empty());
    assert_eq!(parsed.result.next_page_offset, None);
}

#[test]
fn scroll_response_missing_fields_default_empty() {
    let body = json!({ "result": {} });
    let parsed: ScrollResponse = serde_json::from_value(body).expect("valid scroll response");
    assert!(parsed.result.points.is_empty());
    assert_eq!(parsed.result.next_page_offset, None);
}

#[test]
fn scrolled_point_maps_from_raw() {
    let raw = ScrollPointRaw {
        id: json!("chunk-1"),
        payload: json!({"chunk_index": 0}),
    };
    let point = QdrantScrolledPoint {
        id: raw.id,
        payload: raw.payload,
    };
    assert_eq!(point.id, json!("chunk-1"));
    assert_eq!(point.payload["chunk_index"], json!(0));
}
