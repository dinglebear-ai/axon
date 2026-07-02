//! Live round-trip tests against a real Qdrant instance.
//!
//! These are `#[ignore]`d because they require a reachable Qdrant. Point them at
//! one with `AXON_TEST_QDRANT_URL` (defaults to `http://127.0.0.1:6333`) and run:
//!
//! ```bash
//! AXON_TEST_QDRANT_URL=http://127.0.0.1:6333 \
//!   cargo test -p axon-vectors --lib -- --ignored qdrant_live
//! ```
//!
//! They validate the observable behavior the `FakeVectorStore` oracle defines:
//! ensure/upsert/search plus the generation-visibility contract (uncommitted
//! points stay invisible to a committed-generation search until publish).

use axon_api::source::*;

use crate::point::VectorPointBatchBuilder;
use crate::qdrant::QdrantVectorStore;
use crate::store::VectorStore;
use crate::testing::{
    test_collection_spec, test_embedding_result_for, test_prepared_document,
    test_vector_build_context,
};

fn live_url() -> String {
    std::env::var("AXON_TEST_QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".to_string())
}

fn unique_collection(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

async fn cleanup(store: &QdrantVectorStore, collection: &str) {
    let _ = store
        .delete(VectorDeleteSelector::Filter {
            collection: collection.to_string(),
            filter: serde_json::json!({}),
        })
        .await;
}

#[tokio::test]
#[ignore = "requires a live Qdrant (set AXON_TEST_QDRANT_URL)"]
async fn qdrant_live_ensure_upsert_search_round_trip() {
    let store = QdrantVectorStore::new(live_url(), "qdrant-live");
    let collection = unique_collection("axon-live-search");

    let mut spec = test_collection_spec(3);
    spec.collection = collection.clone();
    spec.dense.name = "dense".to_string();
    store.ensure_collection(spec.clone()).await.unwrap();

    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.collection = collection.clone();

    let write = store.upsert(batch.clone()).await.unwrap();
    assert_eq!(write.points_written, batch.points.len() as u64);

    let search = VectorSearchRequest {
        collection: collection.clone(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: Some(batch.points[0].vector.clone()),
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: Some(false),
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };
    let results = store.search(search).await.unwrap();
    assert!(
        results
            .results
            .iter()
            .any(|hit| hit.point_id == batch.points[0].point_id),
        "upserted point should be retrievable"
    );

    cleanup(&store, &collection).await;
}

#[tokio::test]
#[ignore = "requires a live Qdrant (set AXON_TEST_QDRANT_URL)"]
async fn qdrant_live_generation_becomes_visible_only_after_commit() {
    let store = QdrantVectorStore::new(live_url(), "qdrant-live");
    let collection = unique_collection("axon-live-gen");

    let mut spec = test_collection_spec(3);
    spec.collection = collection.clone();
    spec.dense.name = "dense".to_string();
    store.ensure_collection(spec.clone()).await.unwrap();

    let document = test_prepared_document();
    let source_id = SourceId::new(document.source_id.0.clone());
    let generation = SourceGenerationId::new(document.generation.0.clone());
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.collection = collection.clone();
    let query_vector = batch.points[0].vector.clone();
    store.upsert(batch).await.unwrap();

    let committed_search = |generation: &str| VectorSearchRequest {
        collection: collection.clone(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: Some(query_vector.clone()),
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: Some(false),
        generation: Some(SourceGenerationId::new(generation)),
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    // Before publish, the generation is invisible to a committed search.
    let pre = store.search(committed_search(&generation.0)).await.unwrap();
    assert!(
        pre.results.is_empty(),
        "uncommitted points must be invisible to a committed-generation search"
    );

    store
        .mark_generation_committed(collection.clone(), source_id, generation.clone())
        .await
        .unwrap();

    // After publish, the same committed search now returns the points.
    let post = store.search(committed_search(&generation.0)).await.unwrap();
    assert!(
        !post.results.is_empty(),
        "committed points must be visible to a committed-generation search"
    );

    cleanup(&store, &collection).await;
}
