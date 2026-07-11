//! Debt-entry → `PruneStep`/`PrunePlan` mapping helpers for cleanup-debt
//! draining (split out of `super` to stay under the monolith line cap).
//!
//! These are pure mapping functions: no store I/O, no ledger calls. `super`
//! (the drain loop in `crate::source::prune`) is the only caller.

use axon_api::source::{
    CleanupDebt, CleanupDebtId, CleanupDebtKind, CleanupSelector, JobId, PrunePlan, PruneStep,
    PruneTargetKind, SourceGenerationId, SourceId, VectorDeleteSelector,
};
use uuid::Uuid;

/// Name the specific, current reason a debt kind cannot be drained yet. Kept
/// in one place so the reasons stay in sync with the prerequisites named in
/// the pruning contract's "Cleanup Debt Execution" section and don't drift
/// into a vague "not wired" blanket excuse.
///
/// Only `ArtifactDelete`/`JobRetention`/`CachePrune` reach the skip path in
/// `drain_one_debt` today — `VectorDelete`/`LedgerPrune`/`GraphPrune`/
/// `MemoryPrune` all drain via [`super::drain_via_executor`].
///
/// Followups (tracked against out-of-territory crates, not yet beaded
/// individually):
/// - `ArtifactDelete`: no durable `ArtifactStore` exists in this codebase yet
///   (see `docs/pipeline-unification/runtime/pruning-contract.md`'s "artifact
///   deletes" ownership row) — there is nothing to call.
/// - `JobRetention`: `axon-jobs`' `cleanup_jobs`/`clear_jobs` are real but are
///   bulk, age/kind-scoped operations, and `axon-jobs` is out of this
///   module's territory — draining `CleanupSelector::JobRows { job_ids }`
///   needs a per-job-id delete added there.
/// - `CachePrune`: no `CacheStore` boundary exists in this codebase.
pub(super) fn skip_reason_for_kind(kind: CleanupDebtKind) -> &'static str {
    match kind {
        CleanupDebtKind::VectorDelete
        | CleanupDebtKind::LedgerPrune
        | CleanupDebtKind::GraphPrune
        | CleanupDebtKind::MemoryPrune => "drained (should not reach the skip path)",
        CleanupDebtKind::ArtifactDelete => "no ArtifactStore exists yet",
        CleanupDebtKind::JobRetention => {
            "axon-jobs cleanup is bulk age/kind-scoped; no per-job-id delete exists (out of territory)"
        }
        CleanupDebtKind::CachePrune => "no CacheStore boundary exists yet",
    }
}

/// Map a debt entry to a single prune step carrying whichever identity its
/// kind needs: `VectorDelete`/`LedgerPrune` use `PruneStep`'s
/// `vector_selector`/`source_id`+`generation` fields; `GraphPrune`/
/// `MemoryPrune` use the per-item identity fields `graph_stable_keys`/
/// `memory_ids` so both route through the same `PruneExecutor`. Returns
/// `None` for any other kind, or when the selector doesn't carry the
/// identity the kind needs.
pub(super) fn debt_to_step(debt: &CleanupDebt) -> Option<PruneStep> {
    match debt.kind {
        CleanupDebtKind::VectorDelete => {
            let (source_id, generation) = debt_scope(debt)?;
            Some(PruneStep {
                target: PruneTargetKind::Vector,
                description: format!(
                    "delete superseded vector points for debt {}",
                    debt.debt_id.0
                ),
                estimated_deletes: 1,
                vector_selector: Some(VectorDeleteSelector::Generation {
                    collection: "axon".to_string(),
                    source_id: source_id.clone(),
                    generation: generation.clone(),
                }),
                source_id: Some(source_id),
                generation: Some(generation),
                graph_stable_keys: None,
                graph_edge_ids: None,
                memory_ids: None,
            })
        }
        CleanupDebtKind::LedgerPrune => {
            let CleanupSelector::LedgerGenerations {
                source_id,
                up_to_generation,
            } = &debt.selector
            else {
                return None;
            };
            Some(PruneStep {
                target: PruneTargetKind::Ledger,
                description: format!(
                    "delete superseded ledger generation rows for debt {}",
                    debt.debt_id.0
                ),
                estimated_deletes: 1,
                vector_selector: None,
                source_id: Some(source_id.clone()),
                generation: Some(up_to_generation.clone()),
                graph_stable_keys: None,
                graph_edge_ids: None,
                memory_ids: None,
            })
        }
        CleanupDebtKind::GraphPrune => {
            let CleanupSelector::GraphNodes { stable_keys } = &debt.selector else {
                return None;
            };
            Some(PruneStep {
                target: PruneTargetKind::Graph,
                description: format!("delete graph nodes for debt {}", debt.debt_id.0),
                estimated_deletes: stable_keys.len() as u64,
                vector_selector: None,
                source_id: None,
                generation: None,
                graph_stable_keys: Some(stable_keys.clone()),
                graph_edge_ids: None,
                memory_ids: None,
            })
        }
        CleanupDebtKind::MemoryPrune => {
            let CleanupSelector::MemoryRecords { ids } = &debt.selector else {
                return None;
            };
            Some(PruneStep {
                target: PruneTargetKind::Memory,
                description: format!("forget memory records for debt {}", debt.debt_id.0),
                estimated_deletes: ids.len() as u64,
                vector_selector: None,
                source_id: None,
                generation: None,
                graph_stable_keys: None,
                graph_edge_ids: None,
                memory_ids: Some(ids.clone()),
            })
        }
        _ => None,
    }
}

/// Extract the `(source_id, superseded_generation)` a vector-delete debt names.
/// The generation is the *previous* (now stale) generation the debt targets.
fn debt_scope(debt: &CleanupDebt) -> Option<(SourceId, SourceGenerationId)> {
    match &debt.selector {
        CleanupSelector::SourceItem {
            source_id,
            generation,
            ..
        }
        | CleanupSelector::Generation {
            source_id,
            generation,
        } => Some((source_id.clone(), generation.clone())),
        // A source-wide vector delete carries the generation on the debt row.
        CleanupSelector::Source { source_id } => debt
            .generation
            .clone()
            .map(|generation| (source_id.clone(), generation)),
        // Document/Chunk/Artifact selectors are not generation-fenced source
        // deletes; leave them to their owning executor.
        _ => None,
    }
}

/// Wrap a step in a minimal, execution-ordered plan for the executor. The
/// plan's selector names the cleanup-debt entry it drains — this plan only
/// ever executes in this automatic drain, never reviewed as a user-facing
/// dry-run, so `CleanupDebt` (not a fabricated `Generation`/`Source`
/// selector) is the accurate description.
pub(super) fn single_step_plan(step: PruneStep, debt_id: CleanupDebtId) -> PrunePlan {
    PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: axon_api::source::prune::PruneSelector::CleanupDebt { debt_id },
        destructive: true,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![step],
        warnings: Vec::new(),
    }
}
