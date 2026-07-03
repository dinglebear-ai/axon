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

use axon_api::source::ids::JobId;
use axon_api::source::prune::{
    PruneEstimate, PrunePlan, PruneSelector, PruneStep, PruneTargetKind,
};
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
}

impl<S: PruneScopeSource> PrunePlanner<S> {
    pub fn new(scope: S) -> Self {
        Self { scope }
    }

    /// Resolve `selector` into a concrete plan. The returned plan is a
    /// dry-run description: no state is mutated. `steps` are ordered by the
    /// canonical cleanup-debt execution order.
    pub fn resolve(&self, selector: &PruneSelector) -> PrunePlan {
        let estimated = self.scope.estimate(selector);
        let mut steps = build_steps(selector, &estimated);
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
/// what would be deleted.
fn build_steps(selector: &PruneSelector, est: &PruneEstimate) -> Vec<PruneStep> {
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
            steps.push(PruneStep {
                target,
                description: desc.to_string(),
                estimated_deletes: n,
                source_id: source_id.clone(),
                generation: generation.clone(),
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

fn new_job_id() -> JobId {
    JobId::new(Uuid::new_v4())
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
