use axon_api::source::{
    ChunkId, MetadataMap, SparseVector, SparseVectorConfig, SparseVectorModifier,
    VectorDeleteSelector, VectorPointBatch, VectorSearchRequest,
};
use serde_json::json;

use crate::point::VectorPointBatchBuilder;
use crate::store::{FakeVectorMode, FakeVectorStore, VectorStore};
use crate::testing::{
    test_collection_spec, test_collection_spec_hybrid, test_embedding_result_for,
    test_prepared_document, test_vector_build_context,
};

fn batch() -> VectorPointBatch {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    VectorPointBatchBuilder::new(
        test_collection_spec(3),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap()
}

fn search() -> VectorSearchRequest {
    VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "chunk".to_string(),
        limit: 10,
        dense_vector: Some(vec![1.0, 0.0, 0.0]),
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: Some(false),
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn fake_vector_store_can_simulate_partial_failure_and_slow_write() {
    let partial = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);
    partial
        .ensure_collection(test_collection_spec_hybrid(3))
        .await
        .unwrap();
    let err = partial.upsert(batch()).await.unwrap_err();
    assert_eq!(err.code.to_string(), "provider.partial_failure");
    assert_eq!(partial.search(search()).await.unwrap().results.len(), 1);

    let slow = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::SlowWrite);
    slow.ensure_collection(test_collection_spec_hybrid(3))
        .await
        .unwrap();
    let written = slow.upsert(batch()).await.unwrap();
    assert_eq!(written.points_written, 2);
    assert_eq!(slow.calls().await, vec!["ensure_collection", "upsert"]);
}

#[tokio::test]
async fn fake_vector_store_invalid_payload_errors_do_not_echo_raw_discriminators() {
    let raw_visibility = "customer-alpha-supervalue-12345";
    let store = FakeVectorStore::new("fake-vector");
    store
        .ensure_collection(test_collection_spec_hybrid(3))
        .await
        .unwrap();
    let mut batch = batch();
    batch.points[0]
        .payload
        .insert("visibility".to_string(), json!(raw_visibility));

    let err = store.upsert(batch).await.unwrap_err();

    assert_eq!(err.code.to_string(), "vector.invalid_payload");
    assert!(!err.message.contains(raw_visibility));
}

#[tokio::test]
async fn url_delete_selector_matches_canonical_payload_fields() {
    let store = FakeVectorStore::new("fake-vector");
    store
        .ensure_collection(test_collection_spec_hybrid(3))
        .await
        .unwrap();
    let mut batch = batch();
    batch.points[0].payload.insert(
        "source_item_key".to_string(),
        json!("https://example.com/docs/a"),
    );
    batch.points[1]
        .payload
        .get_mut("chunk_locator")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(
            "canonical_uri".to_string(),
            json!("https://example.com/docs/b"),
        );
    store.upsert(batch).await.unwrap();

    let exact = store
        .delete(VectorDeleteSelector::CanonicalUri {
            collection: "axon-test".to_string(),
            canonical_uri: "https://example.com/docs/a".to_string(),
            match_prefix: false,
        })
        .await
        .unwrap();
    let prefix = store
        .delete(VectorDeleteSelector::CanonicalUri {
            collection: "axon-test".to_string(),
            canonical_uri: "https://example.com/docs/".to_string(),
            match_prefix: true,
        })
        .await
        .unwrap();

    assert_eq!((exact.points_deleted, prefix.points_deleted), (1, 1));
}

#[tokio::test]
async fn fake_vector_store_scores_sparse_vectors_and_applies_limit_before_payload_clone() {
    let store = FakeVectorStore::new("fake-vector");
    let mut spec = test_collection_spec(3);
    spec.sparse = Some(SparseVectorConfig {
        name: "bm42".to_string(),
        modifier: SparseVectorModifier::Idf,
    });
    store.ensure_collection(spec).await.unwrap();
    let mut batch = batch();
    batch.points[0].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1, 7],
        values: vec![0.9, 0.1],
    });
    batch.points[1].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[1].chunk_id.clone(),
        indices: vec![1, 7],
        values: vec![0.1, 0.9],
    });
    store.upsert(batch).await.unwrap();

    let mut request = search();
    request.limit = 1;
    request.dense_vector = Some(vec![0.0, 0.0, 0.0]);
    request.sparse_vector = Some(SparseVector {
        chunk_id: ChunkId::new("query"),
        indices: vec![7],
        values: vec![1.0],
    });
    request.hybrid = Some(true);

    let result = store.search(request).await.unwrap();

    assert_eq!(result.results.len(), 1);
    assert_eq!(
        result.results[0].chunk_id,
        Some(ChunkId::new("chunk-web-2"))
    );
}

#[tokio::test]
async fn fake_vector_store_rejects_sparse_vectors_for_dense_only_collections() {
    let store = FakeVectorStore::new("fake-vector");
    store
        .ensure_collection(test_collection_spec(3))
        .await
        .unwrap();
    let mut batch = batch();
    batch.sparse_vectors = Some(vec![SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1],
        values: vec![0.9],
    }]);

    let err = store.upsert(batch).await.unwrap_err();

    assert_eq!(err.code.to_string(), "vector.sparse_not_configured");
}

#[tokio::test]
async fn fake_vector_store_rejects_malformed_sparse_vectors() {
    let store = FakeVectorStore::new("fake-vector");
    let mut spec = test_collection_spec(3);
    spec.sparse = Some(SparseVectorConfig {
        name: "bm42".to_string(),
        modifier: SparseVectorModifier::Idf,
    });
    store.ensure_collection(spec).await.unwrap();
    let mut batch = batch();
    batch.points[0].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1],
        values: vec![0.9, 0.1],
    });

    let err = store.upsert(batch).await.unwrap_err();

    assert_eq!(err.code.to_string(), "vector.invalid_sparse_vector");
}
