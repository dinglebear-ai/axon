use super::*;

use std::sync::Mutex;

use axon_api::source::{
    AdapterRef, ApiError, AuthSnapshot, AuthorityLevel, CleanupDebt, CleanupDebtId,
    CleanupDebtKind, CleanupSelector, CollectionSpec, ConfigSnapshotId, DocumentCounts, ErrorStage,
    GraphCandidate, GraphCandidateProducer, GraphNodeCandidate, GraphNodeId, ItemCounts, ItemKind,
    JobCreateRequest, JobId, JobIntent, JobKind, JobPriority, JobStatusUpdate, LifecycleStatus,
    ManifestItem, MemoryId, MemoryRequest, MemoryScope, MemoryStatus, MemoryType, MetadataMap,
    PipelinePhase, ProviderCapability, PublishGenerationRequest, PublishState, SourceCounts,
    SourceGeneration, SourceGenerationId, SourceId, SourceItemKey, SourceKind, SourceManifest,
    SourceScope, SourceSummary, Timestamp, VectorDeleteSelector, VectorPointBatch,
    VectorSearchRequest, VectorSearchResult, VectorStoreDeleteResult, VectorStoreWriteResult,
};
use axon_graph::store::{FakeGraphStore, GraphStore};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
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
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
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
async fn source_prune_cleanup_debt_uses_configured_collection() {
    let ledger = FakeLedgerStore::new();
    let (previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();

    // Precondition: two pending debts for the removed "old" item — the
    // `VectorDelete` this test targets, plus an auto-emitted `GraphPrune` debt
    // (axon-ledger's `publish_generation` now also runs
    // `record_graph_prune_cleanup_debt` for every genuinely-removed manifest
    // item; see `crates/axon-ledger/src/store/fake/cleanup.rs`). Both target
    // the previous (superseded) generation.
    let before = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    let vector_debt = before
        .iter()
        .find(|debt| debt.kind == CleanupDebtKind::VectorDelete)
        .expect("VectorDelete debt for the removed item");
    assert_eq!(vector_debt.generation.as_ref(), Some(&previous));
    assert!(
        before
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::GraphPrune),
        "auto-emitted GraphPrune debt for the removed item"
    );

    let summary = drain_cleanup_debt(&ledger, &vector, COLLECTION, &index_counts(&committed)).await;

    // The VectorDelete debt resolves; the auto-emitted GraphPrune debt fails
    // closed because `drain_cleanup_debt` never wires a GraphStore (see its
    // doc comment) — not silently skipped or fake-resolved.
    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.failed, 1);
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

    // The VectorDelete debt is drained; the GraphPrune debt remains pending
    // (fail-closed, no GraphStore wired for this call).
    let after = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    assert!(
        !after
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::VectorDelete)
    );
    assert!(
        after
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::GraphPrune)
    );
}

#[tokio::test]
async fn source_prune_ledger_error_fails_closed() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore {
        delete_should_fail: true,
        ..Default::default()
    };

    let summary = drain_cleanup_debt(&ledger, &vector, COLLECTION, &index_counts(&committed)).await;

    assert_eq!(summary.resolved, 0);
    // Both pending debts fail closed: the `VectorDelete` because the
    // recording store simulates a delete failure, and the auto-emitted
    // `GraphPrune` debt (see `record_graph_prune_cleanup_debt`) because
    // `drain_cleanup_debt` never wires a GraphStore for this call.
    assert_eq!(summary.failed, 2);
    // Debt is still pending for a later retry — a cleanup failure must not lose
    // the debt. Specifically, the VectorDelete debt under test survives.
    let pending = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
    assert!(
        pending
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::VectorDelete),
        "VectorDelete debt must remain pending after a delete failure"
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

    // Three debts resolve: the pre-existing VectorDelete from
    // `seed_two_generations`, its sibling auto-emitted GraphPrune debt for the
    // same removed "old" item (deleting a stable key with no matching node is
    // a harmless no-op — see `FakeGraphStore::delete_nodes`), and the
    // `GraphPrune` debt for "node1" this test added directly.
    assert_eq!(summary.resolved, 3);
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

    // The memory debt resolves. Graph cleanup fails closed because this call
    // has no graph store, and dependent ledger cleanup remains pending behind
    // that unresolved graph debt.
    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.failed, 2);
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

    // The pre-existing VectorDelete debt from `seed_two_generations` plus the
    // LedgerPrune debt just added both resolve. The auto-emitted GraphPrune
    // debt for the same removed "old" item fails closed — this call passes
    // `graph_store = None`.
    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 1);
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), previous)
            .await
            .unwrap()
            .is_none()
    );
}

fn job_create_request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_prune_test".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: None,
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

fn job_status(job_id: JobId, status: LifecycleStatus, phase: PipelinePhase) -> JobStatusUpdate {
    JobStatusUpdate {
        source_id: None,
        job_id,
        status,
        phase,
        stage_id: None,
        counts: None,
        current: None,
        message: None,
        error: None,
    }
}

/// Create a job and drive it Queued -> Running -> Completed (terminal).
async fn create_terminal_job(store: &FakeJobWatchStore) -> JobId {
    let job = JobStore::create(store, job_create_request()).await.unwrap();
    JobStore::update_status(
        store,
        job_status(
            job.job_id,
            LifecycleStatus::Running,
            PipelinePhase::Embedding,
        ),
    )
    .await
    .unwrap();
    JobStore::update_status(
        store,
        job_status(
            job.job_id,
            LifecycleStatus::Completed,
            PipelinePhase::Complete,
        ),
    )
    .await
    .unwrap();
    job.job_id
}

/// Create a job and drive it Queued -> Running (live — never terminal).
async fn create_running_job(store: &FakeJobWatchStore) -> JobId {
    let job = JobStore::create(store, job_create_request()).await.unwrap();
    JobStore::update_status(
        store,
        job_status(
            job.job_id,
            LifecycleStatus::Running,
            PipelinePhase::Embedding,
        ),
    )
    .await
    .unwrap();
    job.job_id
}

#[tokio::test]
async fn drain_full_deletes_job_rows_when_job_store_wired() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();
    let jobs = FakeJobWatchStore::new();

    let job_id = create_terminal_job(&jobs).await;
    assert!(JobStore::get(&jobs, job_id).await.unwrap().is_some());

    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::JobRetention,
            CleanupSelector::JobRows {
                job_ids: vec![job_id],
            },
        ))
        .await
        .unwrap();

    let summary = drain_cleanup_debt_full_with_jobs(
        &ledger,
        &vector,
        None,
        None,
        Some(&jobs as &dyn JobStore),
        COLLECTION,
        &index_counts(&committed),
    )
    .await;

    // The pre-existing VectorDelete debt from `seed_two_generations` plus the
    // JobRetention debt just added both resolve. The auto-emitted GraphPrune
    // debt for the same removed "old" item fails closed — this call passes
    // `graph_store = None`.
    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 1);
    assert!(JobStore::get(&jobs, job_id).await.unwrap().is_none());
}

/// Symmetric with `drain_full_leaves_graph_debt_pending_without_graph_store`:
/// a `JobRetention` debt with no `JobStore` wired also fails closed through
/// the executor (never fake-resolved), leaving the debt pending.
#[tokio::test]
async fn drain_full_leaves_job_retention_debt_pending_without_job_store() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::JobRetention,
            CleanupSelector::JobRows {
                job_ids: vec![JobId::new(Uuid::from_u128(555))],
            },
        ))
        .await
        .unwrap();
    let vector = RecordingVectorStore::default();

    // `drain_cleanup_debt` never carries a `JobStore` (see its doc comment),
    // so this exercises the same "no store wired" fail-closed path as the
    // Graph/Memory tests above without needing the `_with_jobs` entry point.
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

/// A `JobRows` debt naming both a terminal and a still-live job: the live
/// job is refused (safety — never delete out from under a running worker)
/// while the terminal one is deleted. `delete_jobs` reports the live row via
/// `skipped_live`, not an error, so — like a partial Graph/Memory delete
/// would not be treated as a store failure — the debt still resolves overall
/// (only an actual store *error* leaves debt pending; see
/// `apply_job_retention`'s doc comment).
#[tokio::test]
async fn drain_full_resolves_job_retention_debt_even_when_a_named_row_is_still_live() {
    let ledger = FakeLedgerStore::new();
    let (_previous, committed) = seed_two_generations(&ledger).await;
    let vector = RecordingVectorStore::default();
    let jobs = FakeJobWatchStore::new();

    let terminal_id = create_terminal_job(&jobs).await;
    let live_id = create_running_job(&jobs).await;

    ledger
        .record_cleanup_debt(cleanup_debt(
            CleanupDebtKind::JobRetention,
            CleanupSelector::JobRows {
                job_ids: vec![terminal_id, live_id],
            },
        ))
        .await
        .unwrap();

    let summary = drain_cleanup_debt_full_with_jobs(
        &ledger,
        &vector,
        None,
        None,
        Some(&jobs as &dyn JobStore),
        COLLECTION,
        &index_counts(&committed),
    )
    .await;

    // The pre-existing VectorDelete debt from `seed_two_generations` plus the
    // JobRetention debt just added both resolve (a still-live named row is a
    // partial, non-error skip — see `apply_job_retention`). The auto-emitted
    // GraphPrune debt for the same removed "old" item fails closed — this
    // call passes `graph_store = None`.
    assert_eq!(summary.resolved, 2);
    assert_eq!(summary.failed, 1);
    assert!(JobStore::get(&jobs, terminal_id).await.unwrap().is_none());
    assert!(
        JobStore::get(&jobs, live_id).await.unwrap().is_some(),
        "a live job must survive the drain even when its debt resolves"
    );
}
