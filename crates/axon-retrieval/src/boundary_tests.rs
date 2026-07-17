use std::sync::Arc;

use axon_api::mcp_schema::AskRequest;
use axon_api::source::{
    BatchId, ChunkId, HealthStatus, MetadataMap, ProviderId, VectorPoint, VectorPointBatch,
    VectorPointId,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_vectors::store::{FakeVectorStore, VectorStore};
use axon_vectors::testing::test_collection_spec_hybrid;
use serde_json::json;
use uuid::Uuid;

use crate::boundary::RetrievalEngine as BoundaryRetrievalEngine;
use crate::engine::{RetrievalAccess, RetrievalEngine, RetrievalEngineConfig};
use crate::query::{QueryRequest, RetrievalRequest};

fn retrieval_config() -> RetrievalEngineConfig {
    RetrievalEngineConfig::new(
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        4,
        RetrievalAccess::standard(),
    )
}

fn request() -> RetrievalRequest {
    RetrievalRequest {
        query: "alpha chunk body".to_string(),
        collection: "axon-test".to_string(),
        limit: 3,
        source_id: None,
        generation: None,
        namespace_filters: Vec::new(),
        excluded_source_kinds: Vec::new(),
        byte_budget: 4096,
        token_budget: 512,
    }
}

fn point(point_id: &str, chunk_id: &str, vector: &[f32], text: &str) -> VectorPoint {
    let mut payload = MetadataMap::new();
    payload.insert("payload_contract_version".to_string(), json!("2026-07-01"));
    payload.insert("collection".to_string(), json!("axon-test"));
    payload.insert("source_family".to_string(), json!("web"));
    payload.insert("source_kind".to_string(), json!("web"));
    payload.insert("source_adapter".to_string(), json!("web"));
    payload.insert("source_scope".to_string(), json!("page"));
    payload.insert(
        "item_canonical_uri".to_string(),
        json!(format!("https://example.com/{chunk_id}")),
    );
    payload.insert(
        "source_canonical_uri".to_string(),
        json!(format!("https://example.com/{chunk_id}")),
    );
    payload.insert(
        "source_item_key".to_string(),
        json!(format!("https://example.com/{chunk_id}")),
    );
    payload.insert("source_id".to_string(), json!("src-docs"));
    payload.insert("source_generation".to_string(), json!(7));
    payload.insert("committed_generation".to_string(), json!(7));
    payload.insert("vector_point_id".to_string(), json!(point_id));
    payload.insert("document_id".to_string(), json!(format!("doc-{chunk_id}")));
    payload.insert("chunk_id".to_string(), json!(chunk_id));
    payload.insert("chunk_index".to_string(), json!(0));
    payload.insert("chunking_profile".to_string(), json!("markdown_sections"));
    payload.insert("chunking_method".to_string(), json!("heading_sections"));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": format!("https://example.com/{chunk_id}"),
            "path": format!("/{chunk_id}"),
            "heading_path": [],
            "symbol": null,
            "range": {
                "line_start": 1,
                "line_end": 3,
            }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({
            "line_start": 1,
            "line_end": 3,
        }),
    );
    payload.insert("visibility".to_string(), json!("internal"));
    payload.insert("redaction_status".to_string(), json!("clean"));
    payload.insert("redaction_version".to_string(), json!("2026-07-16"));
    payload.insert("redacted_field_count".to_string(), json!(0));
    payload.insert("dropped_field_count".to_string(), json!(0));
    payload.insert("detector_count".to_string(), json!(0));
    payload.insert("detector_names".to_string(), json!([]));
    payload.insert(
        "job_id".to_string(),
        json!("00000000-0000-0000-0000-000000000099"),
    );
    payload.insert(
        "embedding_batch_id".to_string(),
        json!("00000000-0000-0000-0000-00000000000c"),
    );
    payload.insert("document_status".to_string(), json!("published"));
    payload.insert("embedding_model".to_string(), json!("fake-embedding"));
    payload.insert("embedding_dimensions".to_string(), json!(4));
    payload.insert("embedding_provider".to_string(), json!("fake-embedding"));
    payload.insert("embedding_profile".to_string(), json!("test"));
    payload.insert("embedded_at".to_string(), json!("2026-07-01T00:00:00Z"));
    payload.insert("vector_namespace".to_string(), json!("docs"));
    payload.insert("content_kind".to_string(), json!("markdown"));
    payload.insert("chunk_content_kind".to_string(), json!("markdown"));
    payload.insert(
        "content_hash".to_string(),
        json!(format!("sha256:content-{chunk_id}")),
    );
    payload.insert(
        "chunk_hash".to_string(),
        json!(format!("sha256:chunk-{chunk_id}")),
    );
    payload.insert("chunk_text".to_string(), json!(text));

    VectorPoint {
        point_id: VectorPointId::new(point_id),
        chunk_id: ChunkId::new(chunk_id),
        vector: vector.to_vec(),
        sparse_vector: None,
        payload,
    }
}

/// Builds a store + engine seeded with one matching point ("chunk-a") so
/// every test below exercises the identical real retrieval path through
/// `axon-vectors`'s `FakeVectorStore` and `axon-embedding`'s
/// `FakeEmbeddingProvider` — not a mocked-out shortcut.
async fn seeded_engine() -> RetrievalEngine<FakeVectorStore, FakeEmbeddingProvider> {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));

    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![point(
                "point-a",
                "chunk-a",
                &[1.0, 0.0, 0.0, 0.0],
                "Alpha chunk body",
            )],
        })
        .await
        .unwrap();

    RetrievalEngine::new(store, provider, retrieval_config())
}

#[tokio::test]
async fn trait_retrieve_matches_inherent_retrieve() {
    let engine = seeded_engine().await;

    let inherent = engine.retrieve(request()).await.unwrap();
    let via_trait = BoundaryRetrievalEngine::retrieve(&engine, request())
        .await
        .unwrap();

    assert_eq!(inherent, via_trait);
    assert_eq!(via_trait.matches.len(), 1);
    assert_eq!(via_trait.matches[0].chunk_id, ChunkId::new("chunk-a"));
}

#[tokio::test]
async fn query_composes_retrieval_into_query_result() {
    let engine = seeded_engine().await;

    let result = engine
        .query(QueryRequest {
            query: "alpha chunk body".to_string(),
            collection: "axon-test".to_string(),
            limit: 3,
            namespace_filters: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].text, "Alpha chunk body");
    assert_eq!(result.citations.len(), 1);
}

#[tokio::test]
async fn build_ask_context_composes_context_bundle_and_citations() {
    let engine = seeded_engine().await;

    let ask_context = engine
        .build_ask_context(AskRequest {
            query: Some("alpha chunk body".to_string()),
            collection: Some("axon-test".to_string()),
            ask_chunk_limit: Some(3),
            ..AskRequest::default()
        })
        .await
        .unwrap();

    assert!(ask_context.context.text.contains("Alpha chunk body"));
    assert_eq!(ask_context.citations.len(), 1);
    assert_eq!(ask_context.retrieval.matches.len(), 1);
}

#[tokio::test]
async fn capabilities_reports_healthy_retrieval_capability() {
    let engine = seeded_engine().await;

    let capability = BoundaryRetrievalEngine::capabilities(&engine)
        .await
        .unwrap();

    assert_eq!(capability.0.owner_crate, "axon-retrieval");
    assert_eq!(capability.0.health, HealthStatus::Healthy);
}

#[test]
fn concrete_engine_satisfies_retrieval_engine_trait_object() {
    fn assert_trait_object(_: Arc<dyn BoundaryRetrievalEngine>) {}

    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let engine: RetrievalEngine<FakeVectorStore, FakeEmbeddingProvider> =
        RetrievalEngine::new(store, provider, retrieval_config());

    assert_trait_object(Arc::new(engine));
}
