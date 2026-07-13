use super::*;
use crate::types::RetrieveOptions;
use axon_core::config::Config;

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

// Coverage for the Qdrant-retrieve-result mapping (previously
// `map_direct_retrieve_preserves_metadata`, testing the now-removed
// `map_direct_retrieve_result` / legacy axon-vector's `DirectRetrieveResult`)
// moved to `retrieve::map_tests` (`retrieve_map_tests.rs`), alongside the new
// `axon_retrieval::retrieve::RetrievedDocument`-based
// `retrieve::map_retrieved_document` it now tests.

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
