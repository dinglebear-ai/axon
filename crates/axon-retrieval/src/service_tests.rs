use std::sync::Arc;

use axon_api::source::{BatchId, ProviderId, VectorPointBatch};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::provider::EmbeddingProvider;
use axon_vectors::store::{FakeVectorStore, VectorStore};
use axon_vectors::testing::{TestPointSpec, test_clean_point, test_collection_spec_hybrid};
use uuid::Uuid;

use super::{QueryServiceRequest, run_query};

const BATCH_ID: &str = "00000000-0000-0000-0000-00000000000c";
const JOB_ID: &str = "00000000-0000-0000-0000-000000000099";

fn point(
    point_id: &str,
    chunk_id: &str,
    vector: &[f32],
    text: &str,
) -> axon_api::source::VectorPoint {
    test_clean_point(TestPointSpec {
        collection: "axon-test",
        point_id,
        chunk_id,
        vector,
        text,
        namespace: "docs",
        batch_id: BATCH_ID,
        model: "fake-embedding",
        dimensions: 4,
        job_id: JOB_ID,
    })
}

/// The public `run_query` entry accepts runtime-held trait objects
/// (`Arc<dyn _>`) and returns mapped hits, proving the boundary compiles and the
/// engine runs a hybrid search end-to-end through the fakes.
#[tokio::test]
async fn run_query_returns_mapped_hits_via_trait_objects() {
    let concrete_store = Arc::new(FakeVectorStore::new("fake-vectors"));
    concrete_store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    concrete_store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(0xc)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![
                point("point-a", "chunk-a", &[1.0, 0.0, 0.0, 0.0], "Alpha body"),
                point("point-b", "chunk-b", &[0.0, 1.0, 0.0, 0.0], "Beta body"),
            ],
        })
        .await
        .unwrap();

    let store: Arc<dyn VectorStore> = concrete_store;
    let provider: Arc<dyn EmbeddingProvider> =
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));

    let result = run_query(
        store,
        provider,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        4,
        QueryServiceRequest {
            query: "alpha beta body".to_string(),
            collection: "axon-test".to_string(),
            limit: 5,
        },
    )
    .await
    .unwrap();

    assert!(!result.hits.is_empty());
    let uris: Vec<_> = result
        .hits
        .iter()
        .map(|hit| hit.canonical_uri.as_str())
        .collect();
    assert!(uris.contains(&"https://example.com/chunk-a"));
    assert!(result.hits.iter().all(|hit| !hit.text.is_empty()));
    assert!(result.hits.iter().all(|hit| !hit.chunk_id.is_empty()));
}
