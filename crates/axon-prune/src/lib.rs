//! `axon-prune` — planned destructive cleanup for the source pipeline.
//!
//! This crate owns the *plan / execute / receipt* half of pruning: resolving a
//! [`PruneSelector`](axon_api::source::prune::PruneSelector) into a reviewable
//! dry-run [`PrunePlan`](axon_api::source::prune::PrunePlan) without mutating
//! state, then executing that plan against store boundaries in cleanup-debt
//! order, generation-fenced and idempotent. It never owns the ledger, graph,
//! memory, artifact, or vector stores — it drives them through the
//! [`executor::PruneTarget`] trait so real wiring lands in a later bead.
//!
//! Contract:
//! - `docs/pipeline-unification/crates/axon-prune/README.md`
//! - `docs/pipeline-unification/runtime/pruning-contract.md`
//!
//! Wire DTOs (`PruneRequest`, `PruneSelector`, `PrunePlan`, `PruneResult`, …)
//! live in [`axon_api::source::prune`]; this crate produces and consumes them.

pub mod debt;
pub mod dedupe;
pub mod executor;
pub mod generation;
pub mod orphan;
pub mod plan;
pub mod receipt;
pub mod safety;
pub mod testing;

pub const CRATE_NAME: &str = "axon-prune";

pub use executor::{PruneExecutor, PruneTarget, StepExecution};
pub use plan::{PrunePlanner, PruneScopeSource};
pub use safety::{PruneAuthz, PruneDenied};

// Re-export the wire DTOs this crate is the producer/consumer of, so callers
// can `use axon_prune::{PrunePlan, PruneResult, ...}` without reaching into
// axon-api directly.
pub use axon_api::source::prune::{
    PruneCounts, PruneEstimate, PrunePlan, PruneRequest, PruneResult, PruneSelector, PruneStep,
    PruneStepResult, PruneTargetKind,
};

#[cfg(test)]
#[path = "dto_tests.rs"]
mod dto_tests;
