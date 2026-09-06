use super::*;

struct FailingDocumentStore;

#[async_trait]
impl FullDocumentStore for FailingDocumentStore {
    async fn retrieve_full_document(
        &self,
        _collection: &str,
        _target: &str,
        _max_points: Option<usize>,
    ) -> Result<RetrievedDocument> {
        Err(ApiError::new(
            "retrieval.test_failure",
            axon_api::source::ErrorStage::Retrieving,
            "retrieve failed for all URL variants",
        ))
    }
}

#[tokio::test]
async fn retrieve_document_propagates_transport_failure_across_all_url_variants() {
    let store = FailingDocumentStore;
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
    assert_eq!(doc.chunk_count, 0);
}
