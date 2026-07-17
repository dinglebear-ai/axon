use super::*;
use axon_retrieval::retrieve::RetrievedDocument;
use axon_vectors::qdrant::{QdrantRetrieveByUrlResult, QdrantScrolledPoint, QdrantUrlVariantError};

fn point(id: &str, chunk_index: i64, text: &str) -> QdrantScrolledPoint {
    QdrantScrolledPoint {
        id: serde_json::json!(id),
        payload: serde_json::json!({ "chunk_index": chunk_index, "chunk_text": text }),
    }
}

fn target_payload_point(id: &str, chunk_index: i64, text: &str) -> QdrantScrolledPoint {
    QdrantScrolledPoint {
        id: serde_json::json!(id),
        payload: serde_json::json!({
            "chunk_index": chunk_index,
            "chunk_text": text,
            "item_canonical_uri": "https://example.com/docs/page",
            "source_canonical_uri": "https://example.com/docs",
            "source_item_key": "https://example.com/docs/page#chunk-0"
        }),
    }
}

#[test]
fn map_retrieved_document_returns_none_for_empty_points() {
    let doc = RetrievedDocument {
        result: QdrantRetrieveByUrlResult {
            requested_url: "https://example.com/docs".to_string(),
            matched_url: None,
            points: Vec::new(),
            max_points: 500,
            truncated: false,
            variant_errors: Vec::new(),
        },
        content: String::new(),
    };
    assert!(map_retrieved_document("https://example.com/docs", doc).is_none());
}

#[test]
fn map_retrieved_document_preserves_metadata() {
    let doc = RetrievedDocument {
        result: QdrantRetrieveByUrlResult {
            requested_url: "example.com/docs".to_string(),
            matched_url: Some("https://example.com/docs".to_string()),
            points: vec![point("p1", 0, "hello"), point("p2", 1, "world")],
            max_points: 2,
            truncated: true,
            variant_errors: vec![QdrantUrlVariantError {
                url: "https://example.com/docs/".to_string(),
                error: "timeout".to_string(),
            }],
        },
        content: "hello\nworld".to_string(),
    };

    let resolved = map_retrieved_document("example.com/docs", doc).expect("points present");

    assert_eq!(resolved.backend, DocumentBackend::Qdrant);
    assert_eq!(resolved.content, "hello\nworld");
    assert_eq!(resolved.chunk_count, 2);
    assert_eq!(
        resolved.matched_url.as_deref(),
        Some("https://example.com/docs")
    );
    assert!(resolved.source_truncated);
    assert_eq!(resolved.variant_errors[0].url, "https://example.com/docs/");
    assert_eq!(resolved.variant_errors[0].error, "timeout");
    assert_eq!(resolved.warnings.len(), 1);
    assert!(resolved.warnings[0].contains("truncated at 2 point(s)"));
    assert!(resolved.warnings[0].contains("https://example.com/docs"));
}

#[test]
fn map_retrieved_document_no_warning_when_not_truncated() {
    let doc = RetrievedDocument {
        result: QdrantRetrieveByUrlResult {
            requested_url: "https://example.com/docs".to_string(),
            matched_url: Some("https://example.com/docs".to_string()),
            points: vec![point("p1", 0, "hello")],
            max_points: 500,
            truncated: false,
            variant_errors: Vec::new(),
        },
        content: "hello".to_string(),
    };

    let resolved = map_retrieved_document("https://example.com/docs", doc).expect("points present");
    assert!(resolved.warnings.is_empty());
    assert!(!resolved.source_truncated);
}

#[test]
fn retrieve_works_without_legacy_url_payload() {
    let doc = RetrievedDocument {
        result: QdrantRetrieveByUrlResult {
            requested_url: "https://example.com/docs/page".to_string(),
            matched_url: Some("https://example.com/docs/page".to_string()),
            points: vec![target_payload_point("p1", 0, "target-only payload")],
            max_points: 500,
            truncated: false,
            variant_errors: Vec::new(),
        },
        content: "target-only payload".to_string(),
    };

    let resolved =
        map_retrieved_document("https://example.com/docs/page", doc).expect("points present");

    assert_eq!(resolved.backend, DocumentBackend::Qdrant);
    assert_eq!(resolved.content, "target-only payload");
    assert_eq!(resolved.chunk_count, 1);
    assert_eq!(
        resolved.matched_url.as_deref(),
        Some("https://example.com/docs/page")
    );
    assert!(resolved.warnings.is_empty());
}
