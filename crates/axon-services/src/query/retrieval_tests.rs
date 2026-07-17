use std::sync::Arc;

use axon_api::source::*;
use axon_core::config::Config;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorStore, VectorStore};
use axon_vectors::testing::{TestPointSpec, test_clean_point, test_collection_spec_hybrid};

use super::query_via_retrieval;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;
use crate::types::Pagination;

const BATCH_ID: &str = "00000000-0000-0000-0000-000000000001";
const JOB_ID: &str = "00000000-0000-0000-0000-000000000099";

fn point(point_id: &str, chunk_id: &str, vector: &[f32], text: &str) -> VectorPoint {
    test_clean_point(TestPointSpec {
        collection: "axon-test",
        point_id,
        chunk_id,
        vector,
        text,
        namespace: "docs",
        batch_id: BATCH_ID,
        model: "fake-embedding",
        dimensions: 8,
        job_id: JOB_ID,
    })
}

/// `query_via_retrieval` runs through the retrieval engine using the context's
/// attached target runtime stores and maps hits into `QueryHit`s.
#[tokio::test]
async fn query_via_retrieval_maps_engine_hits_from_ctx_runtime() {
    let mut cfg = Config::test_default();
    cfg.tei_url = "http://tei.invalid".to_string();
    cfg.qdrant_url = "http://qdrant.invalid".to_string();
    cfg.collection = "axon-test".to_string();
    let cfg = Arc::new(cfg);

    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    vectors
        .ensure_collection(test_collection_spec_hybrid(8))
        .await
        .unwrap();
    vectors
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(uuid::Uuid::from_u128(1)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 8,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(8).payload_indexes,
            points: vec![
                point(
                    "point-a",
                    "chunk-a",
                    &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                    "Alpha body",
                ),
                point(
                    "point-b",
                    "chunk-b",
                    &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                    "Beta body",
                ),
            ],
        })
        .await
        .unwrap();

    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let ctx = ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime))
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            Arc::new(FakeJobWatchStore::new()),
            Arc::new(FakeLedgerStore::new()),
            embedder,
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let result = query_via_retrieval(
        &ctx,
        "alpha beta body",
        Pagination {
            limit: 5,
            offset: 0,
        },
    )
    .await
    .unwrap();

    assert!(!result.results.is_empty());
    assert_eq!(result.results[0].rank, 1);
    assert!(result.results.iter().all(|hit| !hit.snippet.is_empty()));
    assert!(
        result
            .results
            .iter()
            .all(|hit| hit.citation.canonical_uri == hit.url)
    );
    assert!(
        result
            .results
            .iter()
            .all(|hit| hit.citation.redaction.visibility == Visibility::Internal)
    );
    assert!(result.results.iter().any(|hit| hit.source == "example.com"));
    assert!(
        result
            .results
            .iter()
            .any(|hit| hit.url == "https://example.com/chunk-a")
    );
}

/// Missing service endpoints error out rather than silently falling back.
#[tokio::test]
async fn query_via_retrieval_errors_without_service_urls() {
    let mut cfg = Config::test_default();
    cfg.tei_url = String::new();
    cfg.qdrant_url = String::new();
    let ctx = ServiceContext::from_runtime(Arc::new(cfg), Arc::new(NoopServiceRuntime));

    let err = query_via_retrieval(
        &ctx,
        "anything",
        Pagination {
            limit: 5,
            offset: 0,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("QDRANT_URL"));
}
