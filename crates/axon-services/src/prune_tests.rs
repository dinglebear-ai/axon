use super::*;

use std::sync::Mutex;

use axon_api::source::ids::{BatchId, JobId, SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneSelector, PruneStep, PruneTargetKind};
use axon_api::source::{
    ApiError, CollectionSpec, ErrorStage, SourceItemKey, VectorPointBatch, VectorStoreDeleteResult,
    VectorStoreWriteResult,
};
use axon_prune::{PruneAuthz, PruneExecutor};
use axon_vectors::store::{FakeVectorStore, Result as VectorResult, VectorStore};
use axon_vectors::testing::{TestPointSpec, test_clean_point, test_collection_spec};
use uuid::Uuid;

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
