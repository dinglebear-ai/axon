//! Cleanup-debt drain for `index_source`.
//!
//! After a source generation is committed, `axon-ledger` has recorded
//! [`CleanupDebt`] rows for every superseded item — vector points that belong to
//! the *previous* generation and are now stale (their point ids embed the old
//! generation, so a re-index writes fresh points and leaves the old ones behind).
//! This module drains that debt: it reads the source's pending debt, runs the
//! real [`axon_prune::PruneExecutor`] against the [`VectorStore`] with
//! generation-fenced deletes, and marks each resolved entry in the ledger.
//!
//! Per the pruning contract, deletes are generation-fenced: the executor refuses
//! to delete the *current committed* generation by accident. The committed
//! generation for the just-published source is passed in as the fence.
//!
//! Failure degrades gracefully — a vector delete error, an unfenced-current
//! collision, or a ledger error is logged and leaves the debt row pending for a
//! later retry. Acquisition never crashes because of a cleanup failure: the
//! source is already acquired, embedded, and published by the time this runs.

use async_trait::async_trait;
use axon_api::source::{
    CleanupDebt, CleanupDebtKind, CleanupSelector, JobId, SourceGenerationId, SourceId,
    VectorDeleteSelector,
};
use axon_ledger::store::LedgerStore;
use axon_prune::{
    PruneExecutor, PrunePlan, PruneStep, PruneTarget, PruneTargetKind, StepExecution,
};
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::result_map::IndexCounts;

/// Outcome of a cleanup-debt drain pass (for logging only — never surfaced on
/// the wire).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DebtDrainSummary {
    /// Debt entries whose steps all resolved and were marked completed.
    pub resolved: u64,
    /// Debt entries left pending (delete failed, fenced, or resolve failed).
    pub failed: u64,
    /// Vector points actually deleted across all resolved entries.
    pub points_deleted: u64,
}

/// Drain pending cleanup debt for the just-published source.
///
/// Reads the source's pending debt from the ledger, executes each entry's
/// generation-fenced vector delete via the prune executor, and marks resolved
/// entries in the ledger. `committed_generation` (the newly published
/// generation) is the fence: no delete may target it.
///
/// Never returns an error — every failure path logs and degrades to leaving the
/// debt pending, so a cleanup problem cannot fail an already-committed index.
pub async fn drain_cleanup_debt(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    let source_id = counts.source_id.clone();
    let committed_generation = counts.generation.clone();

    let pending = match ledger.list_pending_cleanup_debt(source_id.clone()).await {
        Ok(pending) => pending,
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                source_id = %source_id.0,
                "failed to list pending cleanup debt; skipping drain"
            );
            return DebtDrainSummary::default();
        }
    };
    if pending.is_empty() {
        return DebtDrainSummary::default();
    }

    let target = LedgerPruneTarget {
        vector_store,
        collection: collection.to_string(),
        source_id: source_id.clone(),
        committed_generation,
    };
    let executor = PruneExecutor::new(target);

    let mut summary = DebtDrainSummary::default();
    for debt in pending {
        drain_one_debt(ledger, &executor, &debt, &mut summary).await;
    }

    tracing::debug!(
        source_id = %source_id.0,
        resolved = summary.resolved,
        failed = summary.failed,
        points_deleted = summary.points_deleted,
        "cleanup debt drain complete"
    );
    summary
}

/// Execute one debt entry and, on clean success, mark it resolved.
async fn drain_one_debt(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(step) = debt_to_step(debt) else {
        // Non-vector debt kinds are not yet wired here (artifact/graph/memory/
        // ledger/job/cache). Leave them pending for their owning executor.
        tracing::debug!(
            debt_id = %debt.debt_id.0,
            kind = ?debt.kind,
            "skipping non-vector cleanup debt (not wired)"
        );
        return;
    };

    let plan = single_step_plan(step);
    let result = match executor.execute(&plan).await {
        Ok(result) => result,
        Err(denied) => {
            // Generation fence / admin / confirmation refusal. Leave pending.
            tracing::warn!(
                debt_id = %debt.debt_id.0,
                reason = %denied,
                "cleanup debt delete refused; leaving pending"
            );
            summary.failed += 1;
            return;
        }
    };

    if result.cleanup_debt_remaining > 0 {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            remaining = result.cleanup_debt_remaining,
            "cleanup debt delete failed partway; leaving pending"
        );
        summary.failed += 1;
        return;
    }

    if let Err(err) = ledger.resolve_cleanup_debt(debt.debt_id.clone()).await {
        tracing::warn!(
            error = %err.message,
            debt_id = %debt.debt_id.0,
            "vector points deleted but failed to mark debt resolved; leaving pending"
        );
        summary.failed += 1;
        return;
    }

    summary.resolved += 1;
    summary.points_deleted += result.deleted_counts.vector_points;
}

/// Map a vector-delete cleanup-debt entry to a single prune step. Returns `None`
/// for debt kinds this drain does not own.
fn debt_to_step(debt: &CleanupDebt) -> Option<PruneStep> {
    if debt.kind != CleanupDebtKind::VectorDelete {
        return None;
    }
    let (source_id, generation) = debt_scope(debt)?;
    Some(PruneStep {
        target: PruneTargetKind::Vector,
        description: format!(
            "delete superseded vector points for debt {}",
            debt.debt_id.0
        ),
        estimated_deletes: 1,
        source_id: Some(source_id),
        generation: Some(generation),
    })
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

/// Wrap a step in a minimal, execution-ordered plan for the executor.
fn single_step_plan(step: PruneStep) -> PrunePlan {
    PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: axon_api::source::prune::PruneSelector::Generation {
            source_id: step.source_id.clone().unwrap_or_else(|| SourceId::new("")),
            generation: step
                .generation
                .clone()
                .unwrap_or_else(|| SourceGenerationId::new("")),
        },
        destructive: true,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![step],
        warnings: Vec::new(),
    }
}

/// [`PruneTarget`] backed by the real vector store. Deletes are scoped to the
/// debt's superseded generation and fenced against the committed generation.
struct LedgerPruneTarget<'a> {
    vector_store: &'a dyn VectorStore,
    collection: String,
    source_id: SourceId,
    committed_generation: SourceGenerationId,
}

#[async_trait]
impl PruneTarget for LedgerPruneTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // The committed generation is the fence for every step in this drain —
        // all steps belong to the one source just published.
        Some(self.committed_generation.clone())
    }

    async fn apply(&self, step: &PruneStep) -> Result<StepExecution, String> {
        let Some(generation) = &step.generation else {
            return Ok(StepExecution::skipped("no generation on step"));
        };
        // Defensive: never delete the committed generation even if fencing was
        // bypassed. The executor already fences, this is belt-and-suspenders.
        if generation == &self.committed_generation {
            return Ok(StepExecution::skipped(
                "refusing to delete committed generation",
            ));
        }
        let deleted = self
            .vector_store
            .delete(VectorDeleteSelector::Generation {
                collection: self.collection.clone(),
                source_id: self.source_id.clone(),
                generation: generation.clone(),
            })
            .await
            .map_err(|err| err.message.clone())?;
        Ok(StepExecution::deleted(deleted.points_deleted))
    }
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
