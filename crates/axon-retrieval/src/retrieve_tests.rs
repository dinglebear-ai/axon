use super::*;

const UNREACHABLE_QDRANT_URL: &str = "http://127.0.0.1:1";

#[tokio::test]
async fn retrieve_document_propagates_transport_failure_across_all_url_variants() {
    let store = QdrantVectorStore::new(UNREACHABLE_QDRANT_URL, "test-retrieve");
    let err = retrieve_document(&store, "axon", "https://example.com/docs", None)
        .await
        .expect_err("an unreachable Qdrant endpoint must fail every URL variant");
    assert!(
        err.to_string()
            .contains("retrieve failed for all URL variants"),
        "unexpected error message: {err}"
    );
}

#[test]
fn retrieved_document_default_is_empty() {
    let doc = RetrievedDocument::default();
    assert!(doc.content.is_empty());
    assert!(doc.result.points.is_empty());
}
