use std::sync::Arc;

use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_vectors::store::FakeVectorStore;

use crate::sqlite::SqliteMemoryStore;
use crate::store::MemoryStore;
use crate::testing::FixedClock;
use crate::vector::{MemoryBatchLimits, MemoryVectorConfig, VectorBackedMemoryStore};

fn request(body: &str) -> MemoryRequest {
    MemoryRequest {
        memory_type: MemoryType::Decision,
        body: body.to_string(),
        confidence: 0.9,
        salience: 0.8,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        title: Some("decision".to_string()),
        tags: Vec::new(),
        links: Vec::new(),
        decay: None,
        embed: true,
        visibility: None,
    }
}

fn service(
    embeddings: Arc<FakeEmbeddingProvider>,
    vectors: Arc<FakeVectorStore>,
) -> VectorBackedMemoryStore {
    let sqlite: Arc<dyn MemoryStore> = Arc::new(
        SqliteMemoryStore::in_memory(Arc::new(FixedClock::at_2026())).expect("open sqlite"),
    );
    VectorBackedMemoryStore::new(
        sqlite,
        embeddings,
        vectors,
        MemoryVectorConfig {
            collection: "axon-test".to_string(),
            embedding_provider_id: ProviderId::new("fake-embedding"),
            embedding_model: "fake-embedding".to_string(),
            embedding_dimensions: 4,
            batch_limits: MemoryBatchLimits::default(),
        },
    )
}

#[tokio::test]
async fn lifecycle_mutations_never_write_vectors_directly() {
    let embeddings = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let store = service(Arc::clone(&embeddings), Arc::clone(&vectors));

    let first = store.remember(request("first memory")).await.unwrap();
    let second = store.remember(request("second memory")).await.unwrap();
    store
        .update(MemoryUpdateRequest {
            memory_id: first.memory_id.clone(),
            body: Some("updated memory".to_string()),
            title: None,
            memory_type: None,
            confidence: None,
            salience: None,
            scope: None,
            reason: Some("correction".to_string()),
            timestamp: Timestamp("2026-07-16T00:00:01Z".to_string()),
        })
        .await
        .unwrap();
    store
        .supersede(MemorySupersedeRequest {
            memory_id: first.memory_id.clone(),
            replacement_id: second.memory_id.clone(),
            reason: Some("new decision".to_string()),
            timestamp: Timestamp("2026-07-16T00:00:02Z".to_string()),
        })
        .await
        .unwrap();
    store
        .forget(MemoryForgetRequest {
            memory_id: first.memory_id,
            reason: Some("remove".to_string()),
            timestamp: Timestamp("2026-07-16T00:00:03Z".to_string()),
        })
        .await
        .unwrap();

    assert!(embeddings.calls().await.is_empty());
    assert!(vectors.calls().await.is_empty());
}

#[tokio::test]
async fn compact_and_import_delegate_without_generation_zero_publication() {
    let embeddings = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let store = service(Arc::clone(&embeddings), Arc::clone(&vectors));
    let first = store.remember(request("one")).await.unwrap();
    let second = store.remember(request("two")).await.unwrap();

    store
        .compact(MemoryCompactRequest {
            memory_ids: vec![first.memory_id, second.memory_id],
            strategy: "concatenate".to_string(),
            result_type: MemoryType::Fact,
            title: Some("combined".to_string()),
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: true,
            instructions: None,
            timestamp: Timestamp("2026-07-16T00:00:04Z".to_string()),
        })
        .await
        .unwrap();

    assert!(embeddings.calls().await.is_empty());
    assert!(vectors.calls().await.is_empty());
}

#[tokio::test]
async fn empty_query_keeps_sqlite_recall_without_provider_calls() {
    let embeddings = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let store = service(Arc::clone(&embeddings), Arc::clone(&vectors));
    store
        .remember(request("remember canonical pipelines"))
        .await
        .unwrap();

    let result = store
        .search(MemorySearchRequest {
            query: String::new(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
            include_statuses: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.results.len(), 1);
    assert!(embeddings.calls().await.is_empty());
    assert!(vectors.calls().await.is_empty());
}
