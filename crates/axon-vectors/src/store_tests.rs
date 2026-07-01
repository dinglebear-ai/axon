use axon_api::source::*;
use uuid::Uuid;

use crate::store::{FakeVectorStore, VectorStore};

fn collection() -> CollectionSpec {
    CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 3,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: None,
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

fn batch() -> VectorPointBatch {
    VectorPointBatch {
        batch_id: BatchId::new(Uuid::from_u128(10)),
        collection: "axon-test".to_string(),
        points: vec![
            VectorPoint {
                point_id: VectorPointId::new("point-a"),
                chunk_id: ChunkId::new("chunk-a"),
                vector: vec![1.0, 0.0, 0.0],
                sparse_vector: None,
                payload: MetadataMap::new(),
            },
            VectorPoint {
                point_id: VectorPointId::new("point-b"),
                chunk_id: ChunkId::new("chunk-b"),
                vector: vec![0.0, 1.0, 0.0],
                sparse_vector: None,
                payload: MetadataMap::new(),
            },
        ],
        model: "fake-embedding".to_string(),
        dimensions: 3,
        sparse_vectors: None,
        payload_indexes: Vec::new(),
    }
}

#[tokio::test]
async fn fake_vector_store_upserts_searches_and_deletes_without_qdrant() {
    let store = FakeVectorStore::new("fake-vector");

    store.ensure_collection(collection()).await.unwrap();
    let written = store.upsert(batch()).await.unwrap();
    assert_eq!(written.points_written, 2);

    let search = store
        .search(VectorSearchRequest {
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
        })
        .await
        .unwrap();
    assert_eq!(search.results[0].point_id, VectorPointId::new("point-a"));

    let deleted = store
        .delete(VectorDeleteSelector::Chunks {
            collection: "axon-test".to_string(),
            chunk_ids: vec![ChunkId::new("chunk-a")],
        })
        .await
        .unwrap();
    assert_eq!(deleted.points_deleted, 1);
}

#[tokio::test]
async fn fake_vector_store_reports_capabilities_and_records_calls() {
    let store = FakeVectorStore::new("fake-vector");

    let capability = store.capabilities().await.unwrap();
    assert_eq!(capability.provider_kind, ProviderKind::Vector);
    assert!(capability.vector_store.unwrap().dense);

    store.ensure_collection(collection()).await.unwrap();
    store.upsert(batch()).await.unwrap();
    assert_eq!(store.calls().await, vec!["ensure_collection", "upsert"]);

    store.reset().await.unwrap();
    assert!(store.calls().await.is_empty());
}
