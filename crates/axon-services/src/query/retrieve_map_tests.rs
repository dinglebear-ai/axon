use super::*;
use axon_retrieval::retrieve::{RetrieveVariantError, RetrievedDocument};

#[test]
fn map_retrieved_document_returns_none_for_empty_points() {
    let doc = RetrievedDocument {
        requested_url: "https://example.com/docs".to_string(),
        matched_url: None,
        chunk_count: 0,
        max_points: 500,
        truncated: false,
        variant_errors: Vec::new(),
        content: String::new(),
    };
    assert!(map_retrieved_document("https://example.com/docs", doc).is_none());
}

#[test]
fn map_retrieved_document_preserves_metadata() {
    let doc = RetrievedDocument {
        requested_url: "example.com/docs".to_string(),
        matched_url: Some("https://example.com/docs".to_string()),
        chunk_count: 2,
        max_points: 2,
        truncated: true,
        variant_errors: vec![RetrieveVariantError {
            url: "https://example.com/docs/".to_string(),
            error: "timeout".to_string(),
        }],
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
        requested_url: "https://example.com/docs".to_string(),
        matched_url: Some("https://example.com/docs".to_string()),
        chunk_count: 1,
        max_points: 500,
        truncated: false,
        variant_errors: Vec::new(),
        content: "hello".to_string(),
    };

    let resolved = map_retrieved_document("https://example.com/docs", doc).expect("points present");
    assert!(resolved.warnings.is_empty());
    assert!(!resolved.source_truncated);
}

#[test]
fn retrieve_works_without_legacy_url_payload() {
    let doc = RetrievedDocument {
        requested_url: "https://example.com/docs/page".to_string(),
        matched_url: Some("https://example.com/docs/page".to_string()),
        chunk_count: 1,
        max_points: 500,
        truncated: false,
        variant_errors: Vec::new(),
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
