//! Dry-run prune planning: resolve a `PruneSelector` to a concrete, reviewable
//! `PrunePlan` **without mutating any state**.
//!
//! The planner reads impact counts through a [`PruneScopeSource`] abstraction
//! so it is testable with an in-memory fake (`testing::FakeScopeSource`) — it
//! never talks to Qdrant/ledger/graph/memory directly. Steps are always
//! emitted in the cleanup-debt execution order defined by the pruning
//! contract (vector → artifact → graph → memory → ledger → job/cache).
//!
//! See `docs/pipeline-unification/runtime/pruning-contract.md`.

use axon_api::source::ids::{GraphEdgeId, JobId, MemoryId};
use axon_api::source::prune::{
    PruneEstimate, PrunePlan, PruneSelector, PruneStep, PruneTargetKind,
};
use axon_api::source::vector::VectorDeleteSelector;
use uuid::Uuid;

use crate::safety::selector_requires_admin;

/// Read-only view over what a selector *would* touch. Implementations are pure
/// counters — resolving a plan must never delete anything.
pub trait PruneScopeSource {
    /// Estimate the deletion impact of `selector` (counts only).
    fn estimate(&self, selector: &PruneSelector) -> PruneEstimate;
}

/// Resolves selectors into dry-run plans. Stateless; holds a scope source.
pub struct PrunePlanner<S: PruneScopeSource> {
    scope: S,
    /// Active vector-store collection used for `Source`/`Generation`
    /// selectors' vector-delete steps. Defaults to `"axon"` (the historical
    /// hardcoded value) unless overridden via [`Self::with_collection`], so
    /// every existing caller that never threads a real collection through
    /// keeps its prior behavior. `Collection`-selector steps ignore this
    /// entirely — that selector always targets the collection named
    /// explicitly in the request, since a `Collection` prune is allowed to
    /// name any collection, not necessarily the active one.
    collection: String,
}

impl<S: PruneScopeSource> PrunePlanner<S> {
    pub fn new(scope: S) -> Self {
        Self {
            scope,
            collection: "axon".to_string(),
        }
    }

    /// Set the active collection that `Source`/`Generation` vector-delete
    /// steps target. Callers with a real configured collection (e.g.
    /// `axon-services::prune::prune_plan_estimated`, which knows
    /// `ctx.cfg().collection`) must call this — otherwise a plan silently
    /// targets the hardcoded default `"axon"` even when the deployment's
    /// `AXON_COLLECTION` names something else (D4 remediation, PR #418
    /// review).
    pub fn with_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = collection.into();
        self
    }

    /// Resolve `selector` into a concrete plan. The returned plan is a
    /// dry-run description: no state is mutated. `steps` are ordered by the
    /// canonical cleanup-debt execution order.
    pub fn resolve(&self, selector: &PruneSelector) -> PrunePlan {
        let estimated = self.scope.estimate(selector);
        let mut steps = build_steps(selector, &estimated, &self.collection);
        steps.sort_by_key(|s| s.target.order_rank());

        PrunePlan {
            job_id: new_job_id(),
            selector: selector.clone(),
            destructive: true,
            requires_admin: selector_requires_admin(selector),
            estimated,
            steps,
            warnings: Vec::new(),
        }
    }
}

/// Emit the concrete steps for a selector from its estimate. Only boundaries
/// with a positive estimate contribute a step, so the plan reflects exactly
/// what would be deleted. `active_collection` is the collection `Source`/
/// `Generation` vector-delete steps target (see [`PrunePlanner::with_collection`]).
fn build_steps(selector: &PruneSelector, est: &PruneEstimate, active_collection: &str) -> Vec<PruneStep> {
    let (source_id, generation) = match selector {
        PruneSelector::Source { source_id } => (Some(source_id.clone()), None),
        PruneSelector::Generation {
            source_id,
            generation,
        } => (Some(source_id.clone()), Some(generation.clone())),
        _ => (None, None),
    };

    let mut steps = Vec::new();
    let mut push = |target: PruneTargetKind, n: u64, desc: &str| {
        if n > 0 {
            let vector_selector = match target {
                PruneTargetKind::Vector => vector_selector_for(selector, active_collection),
                _ => None,
            };
            let (graph_stable_keys, graph_edge_ids) = match target {
                PruneTargetKind::Graph => graph_identity_for(selector),
                _ => (None, None),
            };
            let memory_ids = match target {
                PruneTargetKind::Memory => memory_identity_for(selector),
                _ => None,
            };
            steps.push(PruneStep {
                target,
                description: desc.to_string(),
                estimated_deletes: n,
                vector_selector,
                source_id: source_id.clone(),
                generation: generation.clone(),
                graph_stable_keys,
                graph_edge_ids,
                memory_ids,
            });
        }
    };

    push(
        PruneTargetKind::Vector,
        est.vector_points,
        "delete vector points",
    );
    push(PruneTargetKind::Artifact, est.artifacts, "delete artifacts");
    push(
        PruneTargetKind::Graph,
        est.graph_nodes + est.graph_edges,
        "prune graph",
    );
    push(
        PruneTargetKind::Memory,
        est.memory_records,
        "prune memory records",
    );
    push(
        PruneTargetKind::Ledger,
        est.ledger_generations,
        "prune ledger generations",
    );
    push(
        PruneTargetKind::JobRetention,
        est.jobs,
        "prune retained jobs",
    );
    push(
        PruneTargetKind::Cache,
        est.cache_entries,
        "prune cache entries",
    );
    steps
}

fn vector_selector_for(selector: &PruneSelector, active_collection: &str) -> Option<VectorDeleteSelector> {
    match selector {
        PruneSelector::Source { source_id } => Some(VectorDeleteSelector::Source {
            collection: active_collection.to_string(),
            source_id: source_id.clone(),
            generation: None,
        }),
        PruneSelector::Generation {
            source_id,
            generation,
        } => Some(VectorDeleteSelector::Generation {
            collection: active_collection.to_string(),
            source_id: source_id.clone(),
            generation: generation.clone(),
        }),
        // A `Collection` selector always targets the collection named
        // explicitly in the request — never `active_collection` — since it
        // is allowed to name any collection (e.g. cleaning up a stale one),
        // not necessarily the process's configured default.
        PruneSelector::Collection { collection } => Some(VectorDeleteSelector::Collection {
            collection: collection.clone(),
        }),
        _ => None,
    }
}

/// Extract the `(stable_keys, edge_ids)` a `PruneSelector::Graph` names, so
/// the resulting `Graph` step can route through `PruneExecutor` /
/// `GraphStore::delete_nodes`/`delete_edges`. Other selector kinds carry no
/// per-item graph identity (a `Source`/`Generation` prune has no graph scope
/// today).
fn graph_identity_for(selector: &PruneSelector) -> (Option<Vec<String>>, Option<Vec<GraphEdgeId>>) {
    match selector {
        PruneSelector::Graph { node_id, edge_id } => (
            node_id.as_ref().map(|id| vec![id.0.clone()]),
            edge_id.as_ref().map(|id| vec![id.clone()]),
        ),
        _ => (None, None),
    }
}

/// Extract the memory ids a `PruneSelector::Memory` names, so the resulting
/// `Memory` step can route through `PruneExecutor` / `MemoryStore::forget`.
fn memory_identity_for(selector: &PruneSelector) -> Option<Vec<MemoryId>> {
    match selector {
        PruneSelector::Memory { memory_id } => memory_id.as_ref().map(|id| vec![id.clone()]),
        _ => None,
    }
}

fn new_job_id() -> JobId {
    JobId::new(Uuid::new_v4())
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
