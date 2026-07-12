use super::*;

use std::sync::{Arc, Mutex};

use axon_api::source::ids::{BatchId, JobId, SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneSelector, PruneStep, PruneTargetKind};
use axon_api::source::{
    AdapterRef, ApiError, AuthorityLevel, CollectionSpec, ContentKind, ErrorStage, ItemKind,
    LifecycleStatus, ManifestItem, MetadataMap, ProviderId, PublishGenerationRequest, PublishState,
    SourceCounts, SourceItemKey, SourceKind, SourceManifest, SourceScope, SourceSummary, Timestamp,
    VectorPointBatch, VectorStoreDeleteResult, VectorStoreWriteResult,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_prune::{PruneAuthz, PruneExecutor};
use axon_vectors::store::{FakeVectorStore, Result as VectorResult, VectorStore};
use axon_vectors::testing::{TestPointSpec, test_clean_point, test_collection_spec};
use serde_json::json;
use uuid::Uuid;

use crate::context::TargetLocalSourceRuntime;

fn selector() -> PruneSelector {
    PruneSelector::Source {
        source_id: SourceId::new("owner/repo"),
    }
}

fn dry_run_request() -> PruneRequest {
    PruneRequest::dry_run(selector(), "test dry-run")
}

fn execute_request() -> PruneRequest {
    PruneRequest::execute(selector(), "test exec")
}

fn plan_with_vector_step(destructive_and_admin: bool) -> PrunePlan {
    PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: selector(),
        destructive: destructive_and_admin,
        requires_admin: destructive_and_admin,
        estimated: PruneEstimate::default(),
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "delete vector points".to_string(),
            estimated_deletes: 3,
            vector_selector: Some(VectorDeleteSelector::Source {
                collection: "axon-test".to_string(),
                source_id: SourceId::new("owner/repo"),
                generation: None,
            }),
            source_id: Some(SourceId::new("owner/repo")),
            generation: None,
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    }
}

// ---------------------------------------------------------------------
// `prune_plan` (dry-run) — never mutates, always safe to call.
// ---------------------------------------------------------------------

#[test]
fn plan_resolves_selector_without_touching_any_store() {
    let request = dry_run_request();
    let plan = prune_plan(&request);

    assert_eq!(plan.selector, selector());
    // Source selectors are always destructive/admin-gated per the contract,
    // even though this dry-run plan mutates nothing.
    assert!(plan.destructive);
    assert!(plan.requires_admin);
    // No live count API is wired yet (see module docs) — the estimate is
    // honestly zero rather than fabricated, and steps reflect that (a step is
    // only emitted for a boundary with positive estimated impact).
    assert_eq!(plan.estimated, PruneEstimate::default());
    assert!(plan.steps.is_empty());
}

#[test]
fn plan_is_deterministic_shape_for_repeated_calls() {
    let request = dry_run_request();
    let plan_a = prune_plan(&request);
    let plan_b = prune_plan(&request);

    assert_eq!(plan_a.selector, plan_b.selector);
    assert_eq!(plan_a.estimated, plan_b.estimated);
    assert_eq!(plan_a.steps, plan_b.steps);
}

// ---------------------------------------------------------------------
// `prune` convenience wrapper — dry-run vs execute routing.
// ---------------------------------------------------------------------

#[tokio::test]
async fn prune_wrapper_dry_run_never_calls_execute() {
    let cfg = std::sync::Arc::new(axon_core::config::Config::test_default());
    let runtime: std::sync::Arc<dyn crate::runtime::ServiceJobRuntime> =
        std::sync::Arc::new(crate::test_support::NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, runtime);

    let request = dry_run_request();
    let (plan, result) = prune(&ctx, &request, &PruneAuthz::admin())
        .await
        .expect("dry-run plan never errors");

    assert_eq!(plan.selector, selector());
    assert!(result.is_none(), "dry-run must not execute");
}

#[tokio::test]
async fn prune_wrapper_execute_without_admin_is_denied() {
    let cfg = std::sync::Arc::new(axon_core::config::Config::test_default());
    let runtime: std::sync::Arc<dyn crate::runtime::ServiceJobRuntime> =
        std::sync::Arc::new(crate::test_support::NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, runtime);

    let request = execute_request();
    let err = prune(&ctx, &request, &PruneAuthz::anonymous())
        .await
        .expect_err("non-admin destructive execute must be denied");

    assert!(err.to_string().contains("axon:admin"));
}

// ---------------------------------------------------------------------
// `prune_execute` — the destructive path's own safety gate.
// ---------------------------------------------------------------------

#[tokio::test]
async fn execute_without_confirm_is_rejected() {
    let cfg = std::sync::Arc::new(axon_core::config::Config::test_default());
    let runtime: std::sync::Arc<dyn crate::runtime::ServiceJobRuntime> =
        std::sync::Arc::new(crate::test_support::NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, runtime);

    let plan = plan_with_vector_step(true);
    let err = prune_execute(
        &ctx,
        &plan,
        /* confirm = */ false,
        &PruneAuthz::admin(),
    )
    .await
    .expect_err("missing confirmation must be rejected");

    assert_eq!(err, PruneDenied::ConfirmationRequired);
}

#[tokio::test]
async fn execute_without_admin_scope_is_rejected() {
    let cfg = std::sync::Arc::new(axon_core::config::Config::test_default());
    let runtime: std::sync::Arc<dyn crate::runtime::ServiceJobRuntime> =
        std::sync::Arc::new(crate::test_support::NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, runtime);

    let plan = plan_with_vector_step(true);
    let err = prune_execute(
        &ctx,
        &plan,
        /* confirm = */ true,
        &PruneAuthz::anonymous(),
    )
    .await
    .expect_err("non-admin caller must be rejected on a destructive plan");

    assert_eq!(err, PruneDenied::AdminRequired);
}

#[test]
fn collection_selector_is_no_longer_unsupported() {
    // The guidance-refusal guard `prune_execute` checks before ever touching a
    // store must clear for `Collection` now that it is wired — this is the
    // "un-refuse" half of the change; the "actually deletes" half is
    // `execute_collection_selector_deletes_all_points_and_keeps_collection`
    // below.
    assert!(
        unsupported_selector_guidance(&PruneSelector::Collection {
            collection: "axon".to_string(),
        })
        .is_none(),
        "collection prune should no longer carry unsupported-selector guidance"
    );
}

#[tokio::test]
async fn execute_collection_selector_deletes_all_points_and_keeps_collection() {
    let store = FakeVectorStore::new("fake-vector");
    let spec = test_collection_spec(3);
    store
        .ensure_collection(spec.clone())
        .await
        .expect("ensure_collection");

    let batch_id = Uuid::from_u128(1);
    let point = test_clean_point(TestPointSpec {
        collection: &spec.collection,
        point_id: "p1",
        chunk_id: "c1",
        vector: &[0.1, 0.2, 0.3],
        text: "hello",
        namespace: "dense",
        batch_id: &batch_id.to_string(),
        model: "fake-embedding",
        dimensions: 3,
        job_id: "00000000-0000-0000-0000-000000000000",
    });
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(batch_id),
            collection: spec.collection.clone(),
            points: vec![point],
            model: "fake-embedding".to_string(),
            dimensions: 3,
            sparse_vectors: None,
            payload_indexes: Vec::new(),
        })
        .await
        .expect("seed point");
    assert_eq!(store.points(&spec.collection).await.len(), 1);

    let target = VectorOnlyPruneTarget::new(&store, spec.collection.clone());
    let executor = PruneExecutor::new(target);
    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Collection {
            collection: spec.collection.clone(),
        },
        destructive: true,
        requires_admin: true,
        estimated: PruneEstimate {
            vector_points: 1,
            ..Default::default()
        },
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "delete vector points".to_string(),
            estimated_deletes: 1,
            vector_selector: Some(VectorDeleteSelector::Collection {
                collection: spec.collection.clone(),
            }),
            source_id: None,
            generation: None,
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    };

    let result = executor
        .execute(&plan, &PruneAuthz::admin())
        .await
        .expect("collection prune executes, not refused");

    assert_eq!(result.deleted_counts.vector_points, 1);
    assert!(
        store.points(&spec.collection).await.is_empty(),
        "collection should be emptied by the prune"
    );
    // Collection-wide prune keeps the (now-empty) collection — distinct from
    // `axon reset`, which also wipes SQLite/job state.
    assert!(store.collection_spec(&spec.collection).await.is_some());
}

// ---------------------------------------------------------------------
// `VectorOnlyPruneTarget` — the real (non-fake) PruneTarget impl, exercised
// directly against a recording VectorStore double so the delete call shape
// is asserted without needing a live Qdrant.
// ---------------------------------------------------------------------

struct RecordingVectorStore {
    deletes: Mutex<Vec<VectorDeleteSelector>>,
}

impl RecordingVectorStore {
    fn new() -> Self {
        Self {
            deletes: Mutex::new(Vec::new()),
        }
    }

    fn recorded(&self) -> Vec<VectorDeleteSelector> {
        self.deletes.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl VectorStore for RecordingVectorStore {
    async fn ensure_collection(&self, _spec: CollectionSpec) -> VectorResult<()> {
        Ok(())
    }

    async fn upsert(&self, _batch: VectorPointBatch) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn mark_generation_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _generation: SourceGenerationId,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn mark_unchanged_items_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _previous_generation: SourceGenerationId,
        _committed_generation: SourceGenerationId,
        _source_item_keys: Vec<SourceItemKey>,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> VectorResult<VectorStoreDeleteResult> {
        let collection = match &selector {
            VectorDeleteSelector::Source { collection, .. }
            | VectorDeleteSelector::Generation { collection, .. }
            | VectorDeleteSelector::Collection { collection, .. }
            | VectorDeleteSelector::Document { collection, .. }
            | VectorDeleteSelector::Chunks { collection, .. }
            | VectorDeleteSelector::Points { collection, .. }
            | VectorDeleteSelector::CanonicalUri { collection, .. }
            | VectorDeleteSelector::Filter { collection, .. } => collection.clone(),
        };
        self.deletes.lock().unwrap().push(selector);
        Ok(VectorStoreDeleteResult {
            collection,
            points_matched: 3,
            points_deleted: 3,
            dry_run: false,
            warnings: Vec::new(),
            metadata: Default::default(),
        })
    }

    async fn search(
        &self,
        _request: axon_api::source::VectorSearchRequest,
    ) -> VectorResult<axon_api::source::VectorSearchResult> {
        unimplemented!("not exercised by prune")
    }

    async fn capabilities(&self) -> VectorResult<axon_api::source::ProviderCapability> {
        unimplemented!("not exercised by prune")
    }
}

struct FailingVectorStore;

#[async_trait::async_trait]
impl VectorStore for FailingVectorStore {
    async fn ensure_collection(&self, _spec: CollectionSpec) -> VectorResult<()> {
        Ok(())
    }

    async fn upsert(&self, _batch: VectorPointBatch) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn mark_generation_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _generation: SourceGenerationId,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn mark_unchanged_items_committed(
        &self,
        _collection: String,
        _source_id: SourceId,
        _previous_generation: SourceGenerationId,
        _committed_generation: SourceGenerationId,
        _source_item_keys: Vec<SourceItemKey>,
    ) -> VectorResult<VectorStoreWriteResult> {
        unimplemented!("not exercised by prune")
    }

    async fn delete(
        &self,
        _selector: VectorDeleteSelector,
    ) -> VectorResult<VectorStoreDeleteResult> {
        Err(ApiError::new(
            "provider.delete_failed",
            ErrorStage::Cleaning,
            "forced failure",
        ))
    }

    async fn search(
        &self,
        _request: axon_api::source::VectorSearchRequest,
    ) -> VectorResult<axon_api::source::VectorSearchResult> {
        unimplemented!("not exercised by prune")
    }

    async fn capabilities(&self) -> VectorResult<axon_api::source::ProviderCapability> {
        unimplemented!("not exercised by prune")
    }
}

#[tokio::test]
async fn vector_only_target_deletes_via_real_selector_on_step() {
    let store = RecordingVectorStore::new();
    let target = VectorOnlyPruneTarget::new(&store, "axon-test");
    let executor = PruneExecutor::new(target);

    let plan = plan_with_vector_step(true);
    let result = executor
        .execute(&plan, &PruneAuthz::admin())
        .await
        .expect("admin+confirmed execute path is not gated by the executor itself");

    assert_eq!(result.deleted_counts.vector_points, 3);
    let recorded = store.recorded();
    assert_eq!(recorded.len(), 1);
    match &recorded[0] {
        VectorDeleteSelector::Source {
            collection,
            source_id,
            ..
        } => {
            assert_eq!(collection, "axon-test");
            assert_eq!(source_id, &SourceId::new("owner/repo"));
        }
        other => panic!("expected Source selector, got {other:?}"),
    }
}

#[tokio::test]
async fn vector_only_target_reports_debt_on_store_failure() {
    let target = VectorOnlyPruneTarget::new(&FailingVectorStore, "axon-test");
    let executor = PruneExecutor::new(target);

    let plan = plan_with_vector_step(true);
    let result = executor
        .execute(&plan, &PruneAuthz::admin())
        .await
        .expect("store-level failure surfaces as a failed step, not a denial");

    assert_eq!(result.deleted_counts.vector_points, 0);
    assert_eq!(result.cleanup_debt_remaining, 3);
}

// ---------------------------------------------------------------------
// Generation fencing — `VectorOnlyPruneTarget::current_generation` must
// read a real committed generation from a wired ledger so the executor's
// generation-fence (`crates/axon-prune/src/executor.rs`) can actually fire.
// Before this fix `current_generation` always returned `None`, so a
// `--generation` prune targeting the live, currently-committed generation of
// a source was never refused (#298 audit remediation, bead axon_rust-ldozg).
// ---------------------------------------------------------------------

fn ledger_source_summary(source_id: &str) -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(source_id),
        canonical_uri: format!("file:///{source_id}"),
        display_name: source_id.to_string(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "test".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::Verified,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 1,
            vector_points_total: 1,
            bytes_total: 12,
        },
        created_at: Timestamp::from(chrono::Utc::now()),
        updated_at: Timestamp::from(chrono::Utc::now()),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        tags: Vec::new(),
        user_label: None,
    }
}

fn ledger_manifest(source_id: &str, generation: &SourceGenerationId) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new(source_id),
        generation: generation.clone(),
        adapter: AdapterRef {
            name: "test".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items: vec![ManifestItem {
            source_id: SourceId::new(source_id),
            source_item_key: SourceItemKey::new("item-1"),
            canonical_uri: format!("file:///{source_id}/item-1"),
            item_kind: ItemKind::LocalFile,
            content_kind: Some(ContentKind::Code),
            display_path: Some("item-1".to_string()),
            parent_key: None,
            size_bytes: Some(12),
            content_hash: Some("hash-1".to_string()),
            mtime: Some(Timestamp::from(chrono::Utc::now())),
            version: None,
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        }],
        created_at: Timestamp::from(chrono::Utc::now()),
        metadata: MetadataMap::new(),
    }
}

/// Register `source_id` in a fresh `FakeLedgerStore` and commit its first
/// generation end to end (create → manifest → complete → publish), returning
/// the ledger plus the real, ledger-assigned committed generation id.
async fn ledger_with_committed_generation(
    source_id: &str,
) -> (FakeLedgerStore, SourceGenerationId) {
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(ledger_source_summary(source_id))
        .await
        .expect("register source");
    let created = ledger
        .create_generation(SourceId::new(source_id))
        .await
        .expect("create generation");
    ledger
        .put_manifest(ledger_manifest(source_id, &created.generation))
        .await
        .expect("put manifest");
    // `complete_generation` validates the *caller-supplied* record is already
    // marked done (mirrors `axon-ledger`'s own `completed_generation()` test
    // helper) — `create_generation` alone leaves status `Running`.
    let mut to_complete = created;
    to_complete.status = LifecycleStatus::Completed;
    to_complete.publish_state = PublishState::Writing;
    let completed = ledger
        .complete_generation(to_complete)
        .await
        .expect("complete generation");
    let published = ledger
        .publish_generation(PublishGenerationRequest {
            source_id: SourceId::new(source_id),
            generation: completed.generation.clone(),
            expected_previous_generation: None,
        })
        .await
        .expect("publish generation");
    (ledger, published.generation)
}

#[tokio::test]
async fn prune_execute_fences_current_committed_generation_via_ctx_ledger() {
    // End-to-end through `prune_execute`: proves the ledger is actually
    // threaded from `ServiceContext::target_local_source_runtime()` into
    // `VectorOnlyPruneTarget`, not just that the target's own logic is
    // correct in isolation. Safe to run without a live Qdrant — the fence
    // check in `PruneExecutor::execute` runs *before* `target.apply()`, so a
    // fenced step never reaches the real `QdrantVectorStore` this function
    // constructs internally.
    let (ledger, current_generation) = ledger_with_committed_generation("owner/repo").await;

    let cfg = Arc::new(axon_core::config::Config::test_default());
    let service_jobs: Arc<dyn crate::runtime::ServiceJobRuntime> =
        Arc::new(crate::test_support::NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, service_jobs).with_target_local_source_runtime(
        TargetLocalSourceRuntime::new(
            Arc::new(FakeJobWatchStore::new()),
            Arc::new(ledger),
            Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
            Arc::new(FakeVectorStore::new("fake-vector")),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ),
    );

    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Generation {
            source_id: SourceId::new("owner/repo"),
            generation: current_generation.clone(),
        },
        destructive: true,
        requires_admin: true,
        estimated: PruneEstimate {
            vector_points: 1,
            ledger_generations: 1,
            ..Default::default()
        },
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "delete vector points".to_string(),
            estimated_deletes: 1,
            vector_selector: Some(VectorDeleteSelector::Generation {
                collection: "axon".to_string(),
                source_id: SourceId::new("owner/repo"),
                generation: current_generation.clone(),
            }),
            source_id: Some(SourceId::new("owner/repo")),
            generation: Some(current_generation.clone()),
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    };

    let err = prune_execute(&ctx, &plan, /* confirm = */ true, &PruneAuthz::admin())
        .await
        .expect_err("pruning the ledger's currently-committed generation must be fenced");

    assert_eq!(
        err,
        PruneDenied::CurrentGenerationFenced {
            generation: current_generation,
        }
    );
}

#[tokio::test]
async fn vector_only_target_does_not_fence_a_noncurrent_generation() {
    let (ledger, current_generation) = ledger_with_committed_generation("owner/repo").await;
    // A generation id distinct from the ledger's real committed one ("gen_1")
    // — the fence must let a prune of this generation proceed. Vector payload
    // validation requires `source_generation` to be a non-negative integer,
    // so this must parse the same way real `gen_N` ids do (see
    // `axon_vectors::payload::generation_payload_i64`).
    let stale_generation = SourceGenerationId::new("gen_99");
    assert_ne!(stale_generation, current_generation);

    let store = FakeVectorStore::new("fake-vector");
    let spec = test_collection_spec(3);
    store
        .ensure_collection(spec.clone())
        .await
        .expect("ensure_collection");

    let batch_id = Uuid::from_u128(2);
    let mut point = test_clean_point(TestPointSpec {
        collection: &spec.collection,
        point_id: "p-stale",
        chunk_id: "c-stale",
        vector: &[0.1, 0.2, 0.3],
        text: "stale generation chunk",
        namespace: "dense",
        batch_id: &batch_id.to_string(),
        model: "fake-embedding",
        dimensions: 3,
        job_id: "00000000-0000-0000-0000-000000000000",
    });
    // Overwrite the fixture's default source_id/generation so this point
    // matches the `Generation` selector this test prunes.
    point
        .payload
        .insert("source_id".to_string(), json!("owner/repo"));
    point
        .payload
        .insert("source_generation".to_string(), json!(99));
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(batch_id),
            collection: spec.collection.clone(),
            points: vec![point],
            model: "fake-embedding".to_string(),
            dimensions: 3,
            sparse_vectors: None,
            payload_indexes: Vec::new(),
        })
        .await
        .expect("seed point");
    assert_eq!(store.points(&spec.collection).await.len(), 1);

    let target =
        VectorOnlyPruneTarget::with_ledger(&store, spec.collection.clone(), Arc::new(ledger));
    let executor = PruneExecutor::new(target);

    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Generation {
            source_id: SourceId::new("owner/repo"),
            generation: stale_generation.clone(),
        },
        destructive: true,
        requires_admin: true,
        estimated: PruneEstimate {
            vector_points: 1,
            ..Default::default()
        },
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "delete vector points".to_string(),
            estimated_deletes: 1,
            vector_selector: Some(VectorDeleteSelector::Generation {
                collection: spec.collection.clone(),
                source_id: SourceId::new("owner/repo"),
                generation: stale_generation.clone(),
            }),
            source_id: Some(SourceId::new("owner/repo")),
            generation: Some(stale_generation.clone()),
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    };

    let result = executor
        .execute(&plan, &PruneAuthz::admin())
        .await
        .expect("pruning a non-current generation must not be fenced");

    assert_eq!(result.deleted_counts.vector_points, 1);
    assert!(
        store.points(&spec.collection).await.is_empty(),
        "the stale-generation point should have been deleted"
    );
}
