use super::*;
use crate::types::{DocumentBackend, RetrieveOptions};
use axon_core::config::Config;
use axon_vector::ops::qdrant::DirectRetrieveResult;

// ── map_retrieve_result ───────────────────────────────────────────────────

#[test]
fn map_retrieve_zero_chunks_returns_empty() {
    let result = map_retrieve_result(0, "some content".to_string());
    assert_eq!(result.chunk_count, 0);
    assert_eq!(result.content, "");
    assert_eq!(result.requested_url, None);
    assert_eq!(result.backend, None);
}

#[test]
fn map_retrieve_nonzero_chunks() {
    let result = map_retrieve_result(5, "hello".to_string());
    assert_eq!(result.chunk_count, 5);
    assert_eq!(result.content, "hello");
    assert_eq!(result.next_cursor, None);
}

#[test]
fn map_retrieve_result_serializes_legacy_shape_when_metadata_absent() {
    let result = map_retrieve_result(5, "hello".to_string());
    let value = serde_json::to_value(result).expect("retrieve result serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "chunk_count": 5,
            "content": "hello"
        })
    );
}

#[test]
fn map_direct_retrieve_preserves_metadata() {
    let result = map_direct_retrieve_result(DirectRetrieveResult {
        requested_url: "example.com/docs".to_string(),
        matched_url: Some("https://example.com/docs".to_string()),
        chunk_count: 2,
        content: "hello".to_string(),
        truncated: true,
        warnings: vec!["partial result".to_string()],
        variant_errors: vec![axon_vector::ops::qdrant::RetrieveVariantError {
            url: "https://example.com/docs/".to_string(),
            error: "timeout".to_string(),
        }],
    });
    assert_eq!(result.requested_url.as_deref(), Some("example.com/docs"));
    assert_eq!(
        result.matched_url.as_deref(),
        Some("https://example.com/docs")
    );
    assert!(result.truncated);
    assert_eq!(result.warnings, vec!["partial result"]);
    assert_eq!(result.variant_errors[0].url, "https://example.com/docs/");
    assert_eq!(result.backend, Some(DocumentBackend::Qdrant));
}
#[tokio::test]
async fn retrieve_rejects_local_code_urls() {
    let err = retrieve(
        &Config::test_default(),
        "local-code://project/g/1/src%2Flib.rs",
        RetrieveOptions {
            max_points: None,
            cursor: None,
            token_budget: None,
        },
    )
    .await
    .unwrap_err()
    .to_string();
    assert_eq!(
        err,
        "local-code documents are only available through code_search"
    );
}
