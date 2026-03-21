use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct QdrantPayload {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub chunk_text: String,
    #[serde(default)]
    pub text: String,
    pub chunk_index: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantPoint {
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub payload: QdrantPayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantSearchHit {
    #[serde(default)]
    pub id: serde_json::Value,
    pub score: f64,
    #[serde(default)]
    pub payload: QdrantPayload,
}

/// Response from `/points/search` — `result` is a flat array.
#[derive(Debug, Deserialize)]
pub(crate) struct QdrantSearchResponse {
    #[serde(default)]
    pub(crate) result: Vec<QdrantSearchHit>,
}

/// Response from `/points/query` — `result` is `{"points": [...]}`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct QdrantQueryResult {
    #[serde(default)]
    pub(crate) points: Vec<QdrantSearchHit>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QdrantQueryResponse {
    #[serde(default)]
    pub(crate) result: QdrantQueryResult,
}

pub(crate) const RETRIEVE_MAX_POINTS_CEILING: usize = 500;

#[cfg(test)]
mod tests {
    use super::{QdrantPoint, QdrantSearchHit};

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
}
