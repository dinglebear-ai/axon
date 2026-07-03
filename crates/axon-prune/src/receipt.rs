//! Deletion receipts: fold executed step results into `PruneCounts`.
//!
//! Contract requirement: "Receipts include counts, skipped reasons, and
//! source/generation ids." `PruneStepResult` already carries the per-step
//! skipped reason + source/generation ids; this module aggregates the counts
//! per boundary so a `PruneResult` reports an authoritative tally.

use axon_api::source::prune::{PruneCounts, PruneStepResult, PruneTargetKind};

/// Aggregate executed step results into a `PruneCounts` receipt. Skipped steps
/// (which record `deleted == 0` plus a reason) contribute nothing to the
/// counts, so a skipped boundary is visible but not double-counted.
pub fn counts_from_steps(steps: &[PruneStepResult]) -> PruneCounts {
    let mut counts = PruneCounts::default();
    for step in steps {
        match step.target {
            PruneTargetKind::Vector => counts.vector_points += step.deleted,
            PruneTargetKind::Artifact => counts.artifacts += step.deleted,
            PruneTargetKind::Graph => counts.graph_nodes += step.deleted,
            PruneTargetKind::Memory => counts.memory_records += step.deleted,
            PruneTargetKind::Ledger => counts.ledger_generations += step.deleted,
            PruneTargetKind::JobRetention => counts.jobs += step.deleted,
            PruneTargetKind::Cache => counts.cache_entries += step.deleted,
        }
    }
    counts
}

#[cfg(test)]
#[path = "receipt_tests.rs"]
mod tests;
