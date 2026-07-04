//! Cleanup-debt execution ordering.
//!
//! Cleanup debt is recorded by `axon-ledger` and *executed* here. A debt entry
//! fans out to per-boundary work; this module owns the canonical order that
//! work drains in, matching the pruning contract:
//!
//! 1. vector deletes
//! 2. artifact deletes
//! 3. graph prune
//! 4. memory prune
//! 5. ledger prune (last — keeps join metadata available)
//! 6. job/cache retention
//!
//! Re-running a cleanup is idempotent (the executor's `PruneTarget` deletes are
//! no-ops on already-deleted items), so ordering is the only stateful concern
//! here.

use axon_api::source::prune::PruneTargetKind;

/// The canonical cleanup-debt drain order. Mirrors
/// [`PruneTargetKind::EXECUTION_ORDER`] and is the single source of truth for
/// debt fan-out ordering.
pub fn debt_execution_order() -> [PruneTargetKind; 7] {
    PruneTargetKind::EXECUTION_ORDER
}

/// Sort an arbitrary set of debt boundaries into execution order. Idempotent
/// and stable: re-sorting an already-ordered slice is a no-op.
pub fn order_debt_targets(targets: &mut [PruneTargetKind]) {
    targets.sort_by_key(|t| t.order_rank());
}

/// Whether ledger prune is scheduled after every non-ledger boundary present
/// in `targets`. The contract requires ledger to run last so join metadata
/// survives the vector/artifact deletes.
pub fn ledger_runs_last(targets: &[PruneTargetKind]) -> bool {
    match targets.iter().position(|t| *t == PruneTargetKind::Ledger) {
        None => true,
        Some(ledger_idx) => targets
            .iter()
            .enumerate()
            .filter(|(_, t)| **t != PruneTargetKind::Ledger)
            .all(|(i, _)| i < ledger_idx),
    }
}

#[cfg(test)]
#[path = "debt_tests.rs"]
mod tests;
