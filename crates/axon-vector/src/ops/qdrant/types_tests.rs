use super::{QdrantPoint, QdrantQueryResponse, QdrantSearchHit};

#[test]
fn qdrant_query_response_deserializes_nested_points_shape() {
    // `/points/query` returns {"result": {"points": [...]}} — the nested shape that
    // differs from `/points/search`'s flat {"result": [...]}. This test locks in the
    // struct mapping so a future refactor cannot silently break hybrid search again.
    let json = r#"{
        "result": {
            "points": [
                {"id": "abc", "score": 0.95, "payload": {"url": "https://a.com", "chunk_text": "hello"}},
                {"id": "def", "score": 0.80, "payload": {"url": "https://b.com", "chunk_text": "world"}}
            ]
        }
    }"#;
    let resp: QdrantQueryResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.result.points.len(), 2);
    assert_eq!(resp.result.points[0].payload.url, "https://a.com");
    assert!((resp.result.points[0].score - 0.95).abs() < f64::EPSILON);
    assert_eq!(resp.result.points[1].payload.url, "https://b.com");
    assert!((resp.result.points[1].score - 0.80).abs() < f64::EPSILON);
}

#[test]
fn qdrant_query_response_empty_points_deserializes() {
    let json = r#"{"result": {"points": []}}"#;
    let resp: QdrantQueryResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.points.is_empty());
}

#[test]
fn qdrant_query_response_missing_result_field_uses_default() {
    // serde(default) on both QdrantQueryResponse.result and QdrantQueryResult.points
    // means a completely missing result key should not panic — it yields empty points.
    let json = r#"{}"#;
    let resp: QdrantQueryResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.points.is_empty());
}

#[test]
fn qdrant_point_deserializes_with_id() {
    let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "payload": {"url": "https://example.com", "chunk_text": "hello"}}"#;
    let point: QdrantPoint = serde_json::from_str(json).unwrap();
    assert_eq!(
        point.id,
        serde_json::json!("550e8400-e29b-41d4-a716-446655440000")
    );
}

#[test]
fn qdrant_point_deserializes_without_id() {
    let json = r#"{"payload": {"url": "https://example.com"}}"#;
    let point: QdrantPoint = serde_json::from_str(json).unwrap();
    assert!(point.id.is_null());
}

#[test]
fn qdrant_search_hit_deserializes_with_id() {
    let json = r#"{"id": 12345, "score": 0.95, "payload": {"url": "https://example.com"}}"#;
    let hit: QdrantSearchHit = serde_json::from_str(json).unwrap();
    assert_eq!(hit.id, serde_json::json!(12345));
    assert!((hit.score - 0.95).abs() < f64::EPSILON);
}
