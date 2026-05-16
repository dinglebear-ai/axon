use super::*;

#[test]
fn transform_point_converts_unnamed_to_named() {
    let p = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "vector": [1.0_f64, 0.0, 0.0, 0.5],
        "payload": {"chunk_text": "rust async programming", "url": "https://example.com", "chunk_index": 0}
    });
    let result = transform_point(&p).unwrap();
    let dense = &result["vector"]["dense"];
    assert!(dense.is_array());
    let arr = dense.as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert!((arr[0].as_f64().unwrap() - 1.0).abs() < 1e-6);
    let bm42 = &result["vector"]["bm42"];
    assert!(bm42["indices"].is_array());
    assert!(bm42["values"].is_array());
    assert!(!bm42["indices"].as_array().unwrap().is_empty());
    assert_eq!(result["payload"]["url"], "https://example.com");
    assert_eq!(result["payload"]["chunk_index"], 0);
    assert_eq!(result["id"], "550e8400-e29b-41d4-a716-446655440001");
}

#[test]
fn transform_point_empty_chunk_text_yields_empty_sparse() {
    let p = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440002",
        "vector": [0.1_f64, 0.2, 0.3, 0.4],
        "payload": {"chunk_text": "", "url": "https://example.com"}
    });
    let result = transform_point(&p).unwrap();
    let indices = result["vector"]["bm42"]["indices"].as_array().unwrap();
    assert!(
        indices.is_empty(),
        "empty text should produce empty sparse vector"
    );
}

#[test]
fn transform_point_falls_back_to_text_field() {
    let p = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440003",
        "vector": [0.5_f64, 0.5, 0.0, 0.0],
        "payload": {"text": "vector database search", "url": "https://example.com"}
    });
    let result = transform_point(&p).unwrap();
    assert!(result["vector"]["bm42"]["indices"].is_array());
}

#[test]
fn transform_point_missing_vector_returns_error() {
    let p = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440004",
        "payload": {"chunk_text": "some text"}
    });
    assert!(transform_point(&p).is_err());
}

#[test]
fn transform_point_empty_vector_returns_error() {
    let p = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440005",
        "vector": [],
        "payload": {"chunk_text": "some text"}
    });
    assert!(transform_point(&p).is_err());
}
