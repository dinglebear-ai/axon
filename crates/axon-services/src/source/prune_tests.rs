use super::*;

use std::sync::Mutex;

use axon_api::source::{
    AdapterRef, ApiError, AuthorityLevel, CleanupDebt, CleanupDebtId, CleanupDebtKind,
    CleanupSelector, CollectionSpec, DocumentCounts, ErrorStage, GraphCandidate,
    GraphCandidateProducer, GraphNodeCandidate, GraphNodeId, ItemCounts, ItemKind, JobId,
    LifecycleStatus, ManifestItem, MemoryId, MemoryRequest, MemoryScope, MemoryStatus, MemoryType,
    MetadataMap, ProviderCapability, PublishGenerationRequest, PublishState, SourceCounts,
    SourceGeneration, SourceGenerationId, SourceId, SourceItemKey, SourceKind, SourceManifest,
    SourceScope, SourceSummary, Timestamp, VectorDeleteSelector, VectorPointBatch,
    VectorSearchRequest, VectorSearchResult, VectorStoreDeleteResult, VectorStoreWriteResult,
};
use axon_graph::store::{FakeGraphStore, GraphStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_memory::store::{FakeMemoryStore, MemoryStore};
use axon_vectors::store::{Result as VectorResult, VectorStore};
use uuid::Uuid;

const SRC: &str = "src_drain";
const COLLECTION: &str = "axon-drain-test";

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(SRC),
        canonical_uri: "https://example.com/docs".to_string(),
        display_name: "docs".to_string(),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 1,
            vector_points_total: 1,
            bytes_total: 1,
        },
        created_at: ts(),
        updated_at: ts(),
        graph_node_ids: Vec::new(),
        last_refreshed_at: None,
        user_label: None,
        tags: Vec::new(),
        watch_id: None,
        last_job_id: None,
    }
}

fn manifest(generation: &str, items: Vec<(&str, &str)>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new(SRC),
        generation: SourceGenerationId::new(generation),
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Site,
        items: items
            .into_iter()
            .map(|(key, hash)| ManifestItem {
                source_id: SourceId::new(SRC),
                source_item_key: SourceItemKey::new(key),
                canonical_uri: format!("https://example.com/docs/{key}"),
                item_kind: ItemKind::WebPage,
                content_kind: None,
                display_path: Some(key.to_string()),
                parent_key: None,
                size_bytes: Some(10),
                content_hash: Some(hash.to_string()),
                mtime: Some(ts()),
                version: None,
                fetch_plan: None,
                metadata: MetadataMap::new(),
                graph_hints: Vec::new(),
            })
            .collect(),
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn completed(mut generation: SourceGeneration) -> SourceGeneration {
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Writing;
    generation.item_counts = ItemCounts {
        added: 0,
        modified: 0,
        removed: 0,
        unchanged: 0,
        failed: 0,
    };
    generation.document_counts = DocumentCounts {
        discovered: 0,
        prepared: 0,
        embedded: 0,
        published: 0,
        failed: 0,
    };
    generation
}

async fn publish(ledger: &FakeLedgerStore, generation: SourceGeneration) -> SourceGeneration {
    let done = ledger.complete_generation(generation).await.unwrap();
    ledger
        .publish_generation(PublishGenerationRequest {
            source_id: done.source_id.clone(),
            generation: done.generation.clone(),
            expected_previous_generation: done.previous_generation.clone(),
        })
        .await
        .unwrap()
}

/// Seed a ledger with two published generations where `old` is dropped in the
/// second, producing exactly one superseded-item cleanup debt. Returns the
/// `(previous_generation, committed_generation)` pair.
async fn seed_two_generations(
    ledger: &FakeLedgerStore,
) -> (SourceGenerationId, SourceGenerationId) {
    ledger.upsert_source(source()).await.unwrap();
    let gen1 = ledger.create_generation(SourceId::new(SRC)).await.unwrap();
    ledger
        .put_manifest(manifest(
            &gen1.generation.0,
            vec![("index", "same"), ("old", "gone")],
        ))
        .await
        .unwrap();
    publish(ledger, completed(gen1.clone())).await;

    let gen2 = ledger.create_generation(SourceId::new(SRC)).await.unwrap();
    ledger
        .put_manifest(manifest(&gen2.generation.0, vec![("index", "same")]))
        .await
        .unwrap();
    let published = publish(ledger, completed(gen2)).await;
    assert_eq!(published.publish_state, PublishState::CleanupPending);
    (gen1.generation, published.generation)
}

fn index_counts(committed: &SourceGenerationId) -> IndexCounts {
    IndexCounts {
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new(SRC),
        generation: committed.clone(),
        documents_prepared: 1,
        chunks_prepared: 1,
        vector_points_written: 1,
        removed: 1,
        graph_candidates: Vec::new(),
    }
}

/// Recording vector store: captures every delete selector and returns a fixed
/// per-generation delete count. Only `delete` is exercised by the drain path;
/// the other trait methods are unreachable here. This keeps the test decoupled
/// from the vector payload contract while still driving the real prune executor
/// and the generation-fenced selector this module builds.
#[derive(Default)]
struct RecordingVectorStore {
    deletes: Mutex<Vec<VectorDeleteSelector>>,
    delete_should_fail: bool,
}

#[async_trait::async_trait]
impl VectorStore for RecordingVectorStore {
    async fn ensure_collection(&self, _spec: CollectionSpec) -> VectorResult<()> {
        Ok(())
    }

    async fn upsert(&self, _batch: VectorPointBatch) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("upsert is not exercised by the cleanup-debt drain")
    }

    async fn mark_generation_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _generation: SourceGenerationId,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("mark_generation_committed is not exercised by the drain")
    }

    async fn mark_unchanged_items_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _previous_generation: SourceGenerationId,
        _committed_generation: SourceGenerationId,
        _source_item_keys: Vec<SourceItemKey>,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("mark_unchanged_items_committed is not exercised by the drain")
    }

    async fn delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> VectorResult<VectorStoreDeleteResult> {
        if self.delete_should_fail {
            return Err(ApiError::new(
                "provider.delete_failed",
                ErrorStage::Cleaning,
                "recording store delete failure",
            ));
        }
        let collection = match &selector {
            VectorDeleteSelector::Generation { collection, .. } => collection.clone(),
            _ => COLLECTION.to_string(),
        };
        self.deletes.lock().unwrap().push(selector);
        Ok(VectorStoreDeleteResult {
            collection,
            points_matched: 3,
            points_deleted: 3,
            dry_run: false,
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }

    async fn search(&self, _request: VectorSearchRequest) -> VectorResult<VectorSearchResult> {
        unimplemented!("search is not exercised by the drain")
    }

    async fn capabilities(&self) -> VectorResult<ProviderCapability> {
        unimplemented!("capabilities is not exercised by the drain")
    }
}

#[tokio::test]
async fn drains_superseded_generation_debt_with_generation_fenced_delete() {
    let ledger = FakeLedgerStore::new();
    let (previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();

    // Precondition: exactly one pending debt, targeting the previous generation.
    let before = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].generation.as_ref(), Some(&previous));

    let summary = drain_cleanup_debt(&ledger, &vector, COLLECTION, &index_counts(&committed)).await;

    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.points_deleted, 3);

    // The delete is generation-fenced to the PREVIOUS (superseded) generation,
    // never the committed one.
    let deletes = vector.deletes.lock().unwrap();
    assert_eq!(deletes.len(), 1);
    match &deletes[0] {
        VectorDeleteSelector::Generation {
            collection,
            source_id,
            generation,
        } => {
            assert_eq!(collection, COLLECTION);
            assert_eq!(source_id, &SourceId::new(SRC));
            assert_eq!(generation, &previous);
            assert_ne!(generation, &committed);
        }
        other => panic!("expected a Generation delete selector, got {other:?}"),
    }
    drop(deletes);

    // The debt is drained.
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn drain_leaves_debt_pending_when_vector_delete_fails() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore {
        delete_should_fail: true,
        ..Default::default()
    };

    let summary = drain_cleanup_debt(&ledger, &vector, COLLECTION, &index_counts(&committed)).await;

    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.failed, 1);
    // Debt is still pending for a later retry — a cleanup failure must not lose
    // the debt.
    assert_eq!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn drain_is_noop_when_no_pending_debt() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let gen1 = ledger.create_generation(SourceId::new(SRC)).await.unwrap();
    ledger
        .put_manifest(manifest(&gen1.generation.0, vec![("index", "same")]))
        .await
        .unwrap();
    let published = publish(&ledger, completed(gen1)).await;
    let vector = RecordingVectorStore::default();

    let summary = drain_cleanup_debt(
        &ledger,
        &vector,
        COLLECTION,
        &index_counts(&published.generation),
    )
    .await;

    assert_eq!(summary, DebtDrainSummary::default());
    assert!(vector.deletes.lock().unwrap().is_empty());
}

fn cleanup_debt(kind: CleanupDebtKind, selector: CleanupSelector) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!("debt_{kind:?}")),
        job_id: JobId::new(Uuid::from_u128(0)),
        source_id: SourceId::new(SRC),
        generation: None,
        kind,
        selector,
        status: LifecycleStatus::Pending,
        created_at: ts(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

#[tokio::test]
async fn drain_full_deletes_named_graph_nodes_when_graph_store_wired() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();
    let graph = FakeGraphStore::new();

    graph
        .upsert_candidates(vec![GraphCandidate {
            candidate_id: "cand1".to_string(),
            job_id: JobId::new(Uuid::from_u128(0)),
            source_id: SourceId::new(SRC),
            source_item_key: SourceItemKey::new("index"),
            item_canonical_uri: "https://example.com/docs/index".to_string(),
            document_id: None,
            kind: "concept".to_string(),
            merge_key: None,
            producer: GraphCandidateProducer {
                adapter: "web".to_string(),
                parser: None,
                version: "test".to_string(),
            },
            nodes: vec![GraphNodeCandidate {
                node_kind: "concept".to_string(),
                stable_key: "node1".to_string(),
                label: "Node One".to_string(),
                properties: MetadataMap::new(),
            }],
            edges: Vec::new(),
            evidence: Vec::new(),
            confidence: 1.0,
            metadata: MetadataMap::new(),
        }])
        .await
        .unwrap();
    assert!(
        graph
            .get_node(GraphNodeId::new("node1"))
            .await
            .unwrap()
            .is_some()
    );

    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::GraphPrune,
            CleanupSelector::GraphNodes {
                stable_keys: vec!["node1".to_string()],
            },
        ))
        .await
        .unwrap();

    let summary = drain_cleanup_debt_full(
        &ledger,
        &vector,
        Some(&graph as &dyn GraphStore),
        None,
        COLLECTION,
        &index_counts(&committed),
    )
    .await;

    // The pre-existing VectorDelete debt from `seed_two_generations` plus the
    // GraphPrune debt just added: both should resolve.
    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 0);
    assert!(
        graph
            .get_node(GraphNodeId::new("node1"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn drain_full_leaves_graph_debt_pending_without_graph_store() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::GraphPrune,
            CleanupSelector::GraphNodes {
                stable_keys: vec!["node1".to_string()],
            },
        ))
        .await
        .unwrap();
    let vector = RecordingVectorStore::default();

    let summary = drain_cleanup_debt(
        &ledger,
        &vector,
        COLLECTION,
        &index_counts(&SourceGenerationId::new("gen_none")),
    )
    .await;

    // Graph/Memory steps now route through the same `PruneExecutor` as
    // Vector/Ledger (no more direct-store fallback): with no `GraphStore`
    // wired, `LedgerPruneTarget::apply` returns `Err`, the executor reports
    // that step `Failed`, and the debt stays pending — a failed-closed
    // outcome, not a silent skip.
    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.failed, 1);
    assert_eq!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn drain_full_forgets_named_memory_records_when_memory_store_wired() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();
    let memory = FakeMemoryStore::new();

    memory
        .remember(MemoryRequest {
            memory_type: MemoryType::Fact,
            body: "the sky is blue".to_string(),
            confidence: 0.9,
            salience: 0.5,
            scope: MemoryScope {
                kind: "global".to_string(),
                value: "test".to_string(),
            },
            title: None,
            tags: Vec::new(),
            links: Vec::new(),
            decay: None,
            embed: false,
            visibility: None,
        })
        .await
        .unwrap();
    let memory_id = MemoryId::new("mem_1");
    assert!(memory.get(memory_id.clone()).await.unwrap().is_some());

    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::MemoryPrune,
            CleanupSelector::MemoryRecords {
                ids: vec![memory_id.clone()],
            },
        ))
        .await
        .unwrap();

    let summary = drain_cleanup_debt_full(
        &ledger,
        &vector,
        None,
        Some(&memory as &dyn MemoryStore),
        COLLECTION,
        &index_counts(&committed),
    )
    .await;

    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 0);
    let record = memory.get(memory_id).await.unwrap().unwrap();
    assert_eq!(record.status, MemoryStatus::Forgotten);
}

/// Symmetric with `drain_full_leaves_graph_debt_pending_without_graph_store`:
/// a `MemoryPrune` debt with no `MemoryStore` wired also fails closed through
/// the executor (never fake-resolved), leaving the debt pending.
#[tokio::test]
async fn drain_full_leaves_memory_debt_pending_without_memory_store() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::MemoryPrune,
            CleanupSelector::MemoryRecords {
                ids: vec![MemoryId::new("mem_1")],
            },
        ))
        .await
        .unwrap();
    let vector = RecordingVectorStore::default();

    let summary = drain_cleanup_debt(
        &ledger,
        &vector,
        COLLECTION,
        &index_counts(&SourceGenerationId::new("gen_none")),
    )
    .await;

    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.failed, 1);
    assert_eq!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn drain_full_deletes_superseded_ledger_generation_rows() {
    let ledger = FakeLedgerStore::new();
    let (previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();

    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), previous.clone())
            .await
            .unwrap()
            .is_some()
    );

    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::LedgerPrune,
            CleanupSelector::LedgerGenerations {
                source_id: SourceId::new(SRC),
                up_to_generation: previous.clone(),
            },
        ))
        .await
        .unwrap();

    let summary = drain_cleanup_debt_full(
        &ledger,
        &vector,
        None,
        None,
        COLLECTION,
        &index_counts(&committed),
    )
    .await;

    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 0);
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), previous)
            .await
            .unwrap()
            .is_none()
    );
}
