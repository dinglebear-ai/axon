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
            hybrid: true,
            since: None,
            before: None,
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
    assert!(
        result
            .hits
            .iter()
            .all(|hit| hit.citation.chunk_id.0 == hit.chunk_id)
    );
    assert!(
        result
            .hits
            .iter()
            .all(|hit| hit.citation.redaction.visibility == axon_api::source::Visibility::Internal)
    );
}

/// Legacy/cutover indexes can hold an older committed generation of a document
/// whose `retired_epoch` was never stamped; `run_query` keeps only the newest.
#[tokio::test]
async fn run_query_keeps_only_latest_generation_for_same_document() {
    let concrete_store = Arc::new(FakeVectorStore::new("fake-vectors"));
    concrete_store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let mut old = point(
        "point-old",
        "chunk-old",
        &[1.0, 0.0, 0.0, 0.0],
        "historical alpha body",
    );
    old.payload
        .insert("source_generation".to_string(), serde_json::json!(1));
    old.payload
        .insert("committed_generation".to_string(), serde_json::json!(1));
    old.payload
        .insert("document_id".to_string(), serde_json::json!("doc-stable"));
    let mut current = point(
        "point-current",
        "chunk-current",
        &[1.0, 0.0, 0.0, 0.0],
        "current alpha body",
    );
    current
        .payload
        .insert("source_generation".to_string(), serde_json::json!(16));
    current
        .payload
        .insert("committed_generation".to_string(), serde_json::json!(16));
    current
        .payload
        .insert("document_id".to_string(), serde_json::json!("doc-stable"));
    concrete_store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(0xc)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![old, current],
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
            query: "alpha body".to_string(),
            collection: "axon-test".to_string(),
            limit: 2,
            hybrid: false,
            since: None,
            before: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].chunk_id, "chunk-current");
    assert_eq!(result.hits[0].citation.generation.0, "16");
}

/// Plain `query` must not surface memory-source vectors
/// without explicit intent, even though memory and regular documents share
/// one Qdrant collection and vector namespace.
#[tokio::test]
async fn run_query_excludes_memory_source_kind_by_default() {
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
                {
                    let mut point = test_clean_point(TestPointSpec {
                        collection: "axon-test",
                        point_id: "point-mem",
                        chunk_id: "chunk-mem",
                        vector: &[1.0, 0.0, 0.0, 0.0],
                        text: "phase 3b memory: alpha secret plan",
                        namespace: "dense",
                        batch_id: BATCH_ID,
                        model: "fake-embedding",
                        dimensions: 4,
                        job_id: JOB_ID,
                    });
                    point
                        .payload
                        .insert("source_kind".to_string(), serde_json::json!("memory"));
                    point
                },
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
            query: "alpha body".to_string(),
            collection: "axon-test".to_string(),
            limit: 5,
            hybrid: true,
            since: None,
            before: None,
        },
    )
    .await
    .unwrap();

    assert!(!result.hits.is_empty());
    assert!(
        result.hits.iter().all(|hit| hit.chunk_id != "chunk-mem"),
        "memory-source vector must not appear in plain query results"
    );
}
