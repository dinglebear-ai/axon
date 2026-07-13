//! Prune execution: apply a resolved `PrunePlan` against store boundaries in
//! the cleanup-debt execution order, generation-fenced, idempotent.
//!
//! The executor never talks to a concrete store. It drives a [`PruneTarget`]
//! trait (one delete op per boundary) so the real Qdrant/ledger/graph/memory
//! wiring lands in a later bead while this crate stays unit-testable against a
//! `testing::FakePruneTarget`.
//!
//! Execution order (contract §"Debt execution order"):
//! 1. vector deletes
//! 2. artifact deletes
//! 3. graph prune
//! 4. memory prune
//! 5. ledger prune
//! 6. job/cache retention
//!
//! Ledger prune runs last so join metadata stays available while vector
//! points/artifacts are deleted.

use async_trait::async_trait;
use axon_api::source::enums::LifecycleStatus;
use axon_api::source::ids::SourceGenerationId;
use axon_api::source::prune::{
    PrunePlan, PruneResult, PruneStep, PruneStepResult, PruneTargetKind,
};

use crate::receipt::counts_from_steps;
use crate::safety::{PruneAuthz, PruneDenied, fence_generation};

/// One executed delete against a store boundary.
pub struct StepExecution {
    /// How many items were deleted.
    pub deleted: u64,
    /// Set when the boundary was skipped rather than deleted.
    pub skipped_reason: Option<String>,
}

impl StepExecution {
    pub fn deleted(n: u64) -> Self {
        Self {
            deleted: n,
            skipped_reason: None,
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            deleted: 0,
            skipped_reason: Some(reason.into()),
        }
    }
}

/// The store-boundary sink a plan executes against. One method per boundary;
/// each returns how many items it deleted (or a skip). Implementations MUST be
/// idempotent: deleting already-deleted items is a no-op returning `0`.
#[async_trait]
pub trait PruneTarget: Send + Sync {
    /// The current committed generation for `source`, used to fence deletes.
    /// `Ok(None)` when the source is unknown (nothing current to protect).
    /// `Err` when the lookup itself failed (e.g. a ledger read error) — the
    /// executor fails CLOSED on this rather than treating an error the same
    /// as "nothing to protect" (see [`crate::safety::PruneDenied::FenceCheckFailed`]).
    async fn current_generation(
        &self,
        source_id: Option<&str>,
    ) -> Result<Option<SourceGenerationId>, String>;

    /// Apply one plan step. Called in cleanup-debt execution order.
    async fn apply(&self, step: &PruneStep) -> Result<StepExecution, String>;
}

/// Executes a plan against a `PruneTarget`. Also drains any cleanup debt the
/// target reports as remaining after the plan's steps.
pub struct PruneExecutor<T: PruneTarget> {
    target: T,
}

impl<T: PruneTarget> PruneExecutor<T> {
    pub fn new(target: T) -> Self {
        Self { target }
    }

    /// Execute `plan`. Walks steps in the plan's (already execution-ordered)
    /// sequence, generation-fencing any step that names a generation. Partial
    /// failure does not abort the remaining steps — each boundary records its
    /// own status, and the failed count feeds `cleanup_debt_remaining`.
    ///
    /// `authz` is the caller's authorization context and is checked *before*
    /// any mutation happens. Per the pruning contract ("destructive prune
    /// requires `axon:admin`"), a plan with `requires_admin: true` is refused
    /// with [`PruneDenied::AdminRequired`] unless `authz.is_admin` is set.
    /// This is the only code path that actually deletes vector/artifact/
    /// graph/memory/ledger state, so this is the enforcement point for the
    /// contract's admin gate — every caller, including automatic
    /// system-triggered drains, must pass an explicit `PruneAuthz` rather
    /// than have the check silently skipped.
    pub async fn execute(
        &self,
        plan: &PrunePlan,
        authz: &PruneAuthz,
    ) -> Result<PruneResult, PruneDenied> {
        if plan.requires_admin && !authz.is_admin {
            return Err(PruneDenied::AdminRequired);
        }

        // Re-assert ordering defensively so a hand-built plan can't reorder
        // ledger before vector.
        let mut ordered: Vec<&PruneStep> = plan.steps.iter().collect();
        ordered.sort_by_key(|s| s.target.order_rank());

        let mut step_results: Vec<PruneStepResult> = Vec::with_capacity(ordered.len());
        let mut debt_remaining: u64 = 0;

        for step in ordered {
            // Generation fencing: a step that carries an explicit generation
            // must never target the current committed generation.
            if let Some(target_gen) = &step.generation {
                let source_id = step.source_id.as_ref().map(|s| s.0.as_str());
                match self.target.current_generation(source_id).await {
                    Ok(Some(current)) => fence_generation(target_gen, &current)?,
                    Ok(None) => {
                        // Nothing current is known for this source — honestly
                        // nothing to protect, so the fence does not apply.
                    }
                    Err(reason) => {
                        // Fail closed: a fence-check failure (e.g. a ledger
                        // read error) must never be treated as "nothing to
                        // protect". Refuse the whole prune rather than risk
                        // deleting a generation that was never actually
                        // confirmed non-current.
                        return Err(PruneDenied::FenceCheckFailed { reason });
                    }
                }
            }

            let result = match self.target.apply(step).await {
                Ok(exec) => {
                    let status = if exec.skipped_reason.is_some() {
                        LifecycleStatus::Skipped
                    } else {
                        LifecycleStatus::Completed
                    };
                    PruneStepResult {
                        target: step.target,
                        status,
                        deleted: exec.deleted,
                        skipped_reason: exec.skipped_reason,
                        source_id: step.source_id.clone(),
                        generation: step.generation.clone(),
                    }
                }
                Err(err) => {
                    // Partial failure records remaining cleanup debt: the
                    // undeleted boundary is owed cleanup.
                    debt_remaining += step.estimated_deletes;
                    PruneStepResult {
                        target: step.target,
                        status: LifecycleStatus::Failed,
                        deleted: 0,
                        skipped_reason: Some(err),
                        source_id: step.source_id.clone(),
                        generation: step.generation.clone(),
                    }
                }
            };
            step_results.push(result);
        }

        let deleted_counts = counts_from_steps(&step_results);
        let status = overall_status(&step_results);

        Ok(PruneResult {
            job_id: plan.job_id,
            status,
            steps: step_results,
            deleted_counts,
            cleanup_debt_remaining: debt_remaining,
        })
    }
}

/// Roll per-step statuses into an overall lifecycle status.
fn overall_status(steps: &[PruneStepResult]) -> LifecycleStatus {
    let any_failed = steps.iter().any(|s| s.status == LifecycleStatus::Failed);
    let any_completed = steps.iter().any(|s| s.status == LifecycleStatus::Completed);
    if any_failed {
        if any_completed {
            LifecycleStatus::CompletedDegraded
        } else {
            LifecycleStatus::Failed
        }
    } else {
        LifecycleStatus::Completed
    }
}

/// Ensure a plan's steps are in cleanup-debt execution order. Used by callers
/// and tests to assert the ordering invariant.
pub fn steps_in_execution_order(steps: &[PruneStep]) -> bool {
    steps
        .windows(2)
        .all(|w| w[0].target.order_rank() <= w[1].target.order_rank())
}

/// Assert every target kind used in a plan resolves to a rank (no `usize::MAX`
/// sentinel), i.e. is a known boundary.
pub fn all_targets_known(steps: &[PruneStep]) -> bool {
    steps
        .iter()
        .all(|s| PruneTargetKind::EXECUTION_ORDER.contains(&s.target))
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
