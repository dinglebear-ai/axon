use std::sync::Arc;

use axon_api::source::*;
use axon_core::redact::REDACTION_VERSION;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_graph::{FakeGraphStore, GraphStore};
use axon_vectors::store::FakeVectorStore;

use crate::record::SystemClock;
use crate::sqlite::SqliteMemoryStore;
use crate::store::{FakeMemoryStore, MemoryStore};
use crate::testing::FixedClock;
use crate::vector::{
    MEMORY_VECTOR_NAMESPACE, MemoryBatchLimits, MemoryVectorConfig, VectorBackedMemoryStore,
};

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

fn service(vector_store: Arc<FakeVectorStore>) -> VectorBackedMemoryStore {
    VectorBackedMemoryStore::new(
        Arc::new(FakeMemoryStore::new()),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4)),
        vector_store,
        MemoryVectorConfig {
            collection: "axon-test".to_string(),
            embedding_provider_id: ProviderId::new("fake-embedding"),
            embedding_model: "fake-embedding".to_string(),
            embedding_dimensions: 4,
            batch_limits: MemoryBatchLimits::default(),
        },
    )
}

fn memory_graph_candidate(memory_id: &MemoryId) -> GraphCandidate {
    GraphCandidate {
        candidate_id: format!("memory-upsert:{}", memory_id.0),
        job_id: JobId::new(uuid::Uuid::from_u128(1)),
        source_id: SourceId::new("axon-memory"),
        source_item_key: SourceItemKey::new(memory_id.0.clone()),
        item_canonical_uri: format!("memory://{}", memory_id.0),
        document_id: Some(DocumentId::new(memory_id.0.clone())),
        kind: "memory_document".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "axon-memory".to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes: vec![GraphNodeCandidate {
            node_kind: "memory".to_string(),
            stable_key: format!("memory:{}", memory_id.0),
            label: memory_id.0.clone(),
            properties: MetadataMap::new(),
        }],
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 1.0,
        metadata: MetadataMap::new(),
    }
}

/// Fail-closed redaction guard (phase-3b Task 11): the shared redaction
/// boundary blocks a write it cannot safely scan (oversized body — unbounded
/// regex scanning over attacker-controlled text is itself a DoS surface)
/// rather than persisting it unscrubbed. Uses the *real* `SqliteMemoryStore`
/// as the inner store (not `FakeMemoryStore`, which never redacts) so the
/// guard under test is the actual production write path, and asserts the
/// vector decorator never reaches the embed/upsert step because `remember`
/// fails before `self.inner.remember()` returns.
#[tokio::test]
async fn redaction_failure_blocks_memory_vector_write() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let sqlite_store: Arc<dyn MemoryStore> =
        Arc::new(SqliteMemoryStore::in_memory(Arc::new(SystemClock)).expect("store"));
    let service = VectorBackedMemoryStore::new(
        sqlite_store,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4)),
        Arc::clone(&vectors) as Arc<dyn axon_vectors::store::VectorStore>,
        MemoryVectorConfig {
            collection: "axon-test".to_string(),
            embedding_provider_id: ProviderId::new("fake-embedding"),
            embedding_model: "fake-embedding".to_string(),
            embedding_dimensions: 4,
            batch_limits: MemoryBatchLimits::default(),
        },
    );

    let oversized_body = "a".repeat(axon_core::redact::MAX_REDACTABLE_TEXT_BYTES + 1);
    let result = service.remember(request(&oversized_body)).await;

    let err = result.expect_err("oversized body must fail closed");
    assert_eq!(err.code.to_string(), "redaction.failed");
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn remember_writes_memory_vector_payload() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let service = service(Arc::clone(&vectors));

    let result = service
        .remember(request("phase 3b uses qdrant memory"))
        .await
        .unwrap();

    assert_eq!(result.vector_point_ids.len(), 1);
    let points = vectors.points("axon-test").await;
    assert_eq!(points.len(), 1);
    let payload = &points[0].payload;
    assert_eq!(
        payload["vector_namespace"].as_str(),
        Some(MEMORY_VECTOR_NAMESPACE)
    );
    assert_eq!(
        payload["memory_id"].as_str(),
        Some(result.memory_id.0.as_str())
    );
    assert_eq!(payload["memory_status"].as_str(), Some("active"));
    assert_eq!(payload["redaction_status"].as_str(), Some("clean"));
    assert_eq!(
        payload["redaction_version"].as_str(),
        Some(REDACTION_VERSION)
    );
    assert_eq!(payload["redacted_field_count"].as_u64(), Some(0));
    assert_eq!(payload["dropped_field_count"].as_u64(), Some(0));
    assert_eq!(payload["detector_names"].as_array().unwrap().len(), 0);
    assert_eq!(payload["source_kind"].as_str(), Some("memory"));
    assert_eq!(payload["source_adapter"].as_str(), Some("axon-memory"));
    assert_eq!(
        payload["chunking_profile"].as_str(),
        Some("atomic_metadata")
    );
    assert_eq!(payload["chunking_method"].as_str(), Some("atomic_metadata"));
    assert_eq!(payload["content_kind"].as_str(), Some("plain_text"));
    assert_eq!(
        payload["source_canonical_uri"].as_str().unwrap(),
        format!("memory://{}", result.memory_id.0)
    );
}

#[tokio::test]
async fn vector_search_includes_graph_refs_when_mirror_exists() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let graph = Arc::new(FakeGraphStore::new());
    let graph_store: Arc<dyn GraphStore> = graph.clone();
    let service = service(Arc::clone(&vectors)).with_graph_store(graph_store);
    let result = service
        .remember(request("graph backed qdrant memory"))
        .await
        .unwrap();
    graph
        .upsert_candidates(vec![memory_graph_candidate(&result.memory_id)])
        .await
        .unwrap();

    let hits = service
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "qdrant memory".to_string(),
            limit: 10,
            filters: Default::default(),
            include_graph: true,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();

    assert_eq!(hits.results.len(), 1);
    let graph = hits.graph.expect("graph refs");
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].kind, "memory");
    assert_eq!(
        graph.nodes[0].node_id.0,
        format!("memory:{}", result.memory_id.0)
    );
    assert!(graph.warnings.is_empty());
}

#[tokio::test]
async fn forgotten_memory_is_not_recalled_from_vector_namespace() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let service = service(Arc::clone(&vectors));
    let result = service
        .remember(request("durable qdrant memory"))
        .await
        .unwrap();

    service
        .set_status(MemoryStatusRequest {
            memory_id: result.memory_id.clone(),
            status: MemoryStatus::Forgotten,
            reason: Some("test".to_string()),
            timestamp: Timestamp("2026-07-04T00:00:00Z".to_string()),
        })
        .await
        .unwrap();

    let hits = service
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "durable".to_string(),
            limit: 10,
            filters: Default::default(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert!(hits.results.is_empty());
    assert!(vectors.points("axon-test").await.is_empty());
}

/// Wraps a real embedding provider and fails every call at or after
/// `fail_from_call` (1-indexed), so a test can simulate a batch failing
/// partway through a multi-chunk import.
struct FlakyEmbeddingProvider {
    inner: FakeEmbeddingProvider,
    call_count: std::sync::atomic::AtomicUsize,
    fail_from_call: usize,
}

#[async_trait::async_trait]
impl axon_embedding::provider::EmbeddingProvider for FlakyEmbeddingProvider {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        let call = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if call >= self.fail_from_call {
            return Err(ApiError::new(
                "embedding.fake_failure",
                axon_error::ErrorStage::Embedding,
                "flaky provider forced failure",
            ));
        }
        self.inner.embed(batch).await
    }

    async fn capabilities(
        &self,
    ) -> axon_embedding::provider::Result<axon_api::source::ProviderCapability> {
        self.inner.capabilities().await
    }
}

fn record_for_import(memory_id: &str, body: &str) -> MemoryRecord {
    MemoryRecord {
        visibility: Visibility::Internal,
        memory_id: MemoryId::new(memory_id),
        memory_type: MemoryType::Fact,
        status: MemoryStatus::Active,
        body: body.to_string(),
        confidence: 0.8,
        salience: 0.5,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status: MemoryStatus::Active,
            message: "created".to_string(),
            timestamp: Timestamp("2026-07-04T00:00:00Z".to_string()),
        }],
        title: None,
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

#[tokio::test]
async fn memory_import_embeds_created_records_in_configured_batch_size() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let embeddings = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let clock = Arc::new(FixedClock::at_2026());
    let sqlite: Arc<dyn MemoryStore> =
        Arc::new(SqliteMemoryStore::in_memory(clock).expect("open sqlite"));
    let service = VectorBackedMemoryStore::new(
        sqlite,
        embeddings.clone(),
        vectors.clone(),
        MemoryVectorConfig {
            collection: "axon-test".to_string(),
            embedding_provider_id: ProviderId::new("fake-embedding"),
            embedding_model: "fake-embedding".to_string(),
            embedding_dimensions: 4,
            batch_limits: MemoryBatchLimits {
                embed_batch_size: 2,
                ..MemoryBatchLimits::default()
            },
        },
    );

    let records = vec![
        record_for_import("mem_1", "one"),
        record_for_import("mem_2", "two"),
        record_for_import("mem_3", "three"),
    ];
    let result = service
        .import(MemoryImportRequest {
            records,
            mode: MemoryImportMode::Merge,
            dry_run: false,
        })
        .await
        .unwrap();
    assert_eq!(result.created, 3);

    let calls = embeddings.calls().await;
    let batch_sizes: Vec<usize> = calls.iter().map(|batch| batch.items.len()).collect();
    assert_eq!(batch_sizes, vec![2, 1]);
}

#[tokio::test]
async fn partial_vector_failure_sends_affected_memories_to_review() {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let flaky = Arc::new(FlakyEmbeddingProvider {
        inner: FakeEmbeddingProvider::new("fake-embedding", 4),
        call_count: std::sync::atomic::AtomicUsize::new(0),
        fail_from_call: 2,
    });
    let clock = Arc::new(FixedClock::at_2026());
    let sqlite: Arc<dyn MemoryStore> =
        Arc::new(SqliteMemoryStore::in_memory(clock).expect("open sqlite"));
    let service = VectorBackedMemoryStore::new(
        sqlite,
        flaky,
        vectors,
        MemoryVectorConfig {
            collection: "axon-test".to_string(),
            embedding_provider_id: ProviderId::new("fake-embedding"),
            embedding_model: "fake-embedding".to_string(),
            embedding_dimensions: 4,
            batch_limits: MemoryBatchLimits {
                embed_batch_size: 1,
                ..MemoryBatchLimits::default()
            },
        },
    );

    let records = vec![
        record_for_import("mem_a", "first"),
        record_for_import("mem_b", "second"),
    ];
    let result = service
        .import(MemoryImportRequest {
            records,
            mode: MemoryImportMode::Merge,
            dry_run: false,
        })
        .await
        .unwrap();

    assert_eq!(result.created, 2);
    assert_eq!(result.created_ids.len(), 2);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.code == "memory.vector_failed")
    );

    // The first record's embed call succeeds (call 1); the second's fails
    // (call 2 >= fail_from_call) and must be sent to review, not silently
    // lost or left falsely "active" with no vector.
    let second = service
        .get(result.created_ids[1].clone())
        .await
        .unwrap()
        .expect("second record still durable in SQLite");
    assert_eq!(second.status, MemoryStatus::Review);
}
