//! In-memory fakes + fixtures for exercising the planner and executor without
//! any real store. These are the crate's testable targets: `FakeScopeSource`
//! feeds impact counts to the planner, and `FakePruneTarget` is an idempotent
//! in-memory store the executor deletes against.
//!
//! This crate is a clean-break, empty-DB target — the fakes carry no migration
//! or tombstone behavior.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::ids::SourceGenerationId;
use axon_api::source::prune::{PruneEstimate, PruneSelector, PruneStep, PruneTargetKind};

use crate::executor::{PruneTarget, StepExecution};
use crate::plan::PruneScopeSource;

/// A scope source that returns a fixed estimate for every selector.
#[derive(Debug, Clone)]
pub struct FakeScopeSource {
    pub estimate: PruneEstimate,
}

impl FakeScopeSource {
    pub fn new(estimate: PruneEstimate) -> Self {
        Self { estimate }
    }
}

impl PruneScopeSource for FakeScopeSource {
    fn estimate(&self, _selector: &PruneSelector) -> PruneEstimate {
        self.estimate.clone()
    }
}

/// Records one delete the executor applied, in call order.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedStep {
    pub target: PruneTargetKind,
    pub deleted: u64,
}

/// An idempotent in-memory prune target. It tracks per-boundary remaining
/// counts, so re-applying a step after the count is drained deletes `0` — the
/// idempotency the contract requires. Also records the order steps were
/// applied so tests can assert execution order.
pub struct FakePruneTarget {
    /// Remaining deletable items per boundary. Draining to 0 makes re-runs
    /// no-ops.
    remaining: Mutex<BTreeMap<PruneTargetKind, u64>>,
    /// Ordered log of applied deletes.
    applied: Mutex<Vec<AppliedStep>>,
    /// The current committed generation (fenced against). `None` = no source.
    current_generation: Option<SourceGenerationId>,
    /// Force the fence lookup to fail closed.
    fail_current_generation: bool,
    /// Boundaries that should fail on apply (to exercise partial failure).
    fail: Mutex<Vec<PruneTargetKind>>,
}

impl FakePruneTarget {
    /// A target seeded with per-boundary deletable counts.
    pub fn with_counts(counts: BTreeMap<PruneTargetKind, u64>) -> Self {
        Self {
            remaining: Mutex::new(counts),
            applied: Mutex::new(Vec::new()),
            current_generation: None,
            fail_current_generation: false,
            fail: Mutex::new(Vec::new()),
        }
    }

    /// Seed the deletable counts from a `PrunePlan`'s estimated per-step
    /// deletes so the target is consistent with the plan.
    pub fn from_steps(steps: &[PruneStep]) -> Self {
        let mut counts: BTreeMap<PruneTargetKind, u64> = BTreeMap::new();
        for s in steps {
            *counts.entry(s.target).or_default() += s.estimated_deletes;
        }
        Self::with_counts(counts)
    }

    /// Set the current committed generation used for fencing.
    pub fn with_current_generation(mut self, generation: SourceGenerationId) -> Self {
        self.current_generation = Some(generation);
        self
    }

    /// Force `current_generation()` to return an error.
    pub fn failing_current_generation(mut self) -> Self {
        self.fail_current_generation = true;
        self
    }

    /// Force a boundary to fail on apply (partial-failure testing).
    pub fn failing(self, target: PruneTargetKind) -> Self {
        self.fail.lock().unwrap().push(target);
        self
    }

    /// The ordered log of applied deletes.
    pub fn applied_log(&self) -> Vec<AppliedStep> {
        self.applied.lock().unwrap().clone()
    }

    /// Remaining deletable items for a boundary (0 once drained).
    pub fn remaining_for(&self, target: PruneTargetKind) -> u64 {
        *self.remaining.lock().unwrap().get(&target).unwrap_or(&0)
    }
}

#[async_trait]
impl PruneTarget for FakePruneTarget {
    async fn current_generation(
        &self,
        _source_id: Option<&str>,
    ) -> Result<Option<SourceGenerationId>, String> {
        if self.fail_current_generation {
            return Err("forced current_generation failure".to_string());
        }
        Ok(self.current_generation.clone())
    }

    async fn apply(&self, step: &PruneStep) -> Result<StepExecution, String> {
        if self.fail.lock().unwrap().contains(&step.target) {
            return Err(format!("boundary {:?} forced failure", step.target));
        }

        let mut remaining = self.remaining.lock().unwrap();
        let entry = remaining.entry(step.target).or_default();
        // Idempotent: only delete what still remains, capped by the requested
        // estimate. A drained boundary deletes 0.
        let deletable = (*entry).min(step.estimated_deletes);
        *entry -= deletable;
        drop(remaining);

        self.applied.lock().unwrap().push(AppliedStep {
            target: step.target,
            deleted: deletable,
        });

        if deletable == 0 {
            Ok(StepExecution::skipped("already drained"))
        } else {
            Ok(StepExecution::deleted(deletable))
        }
    }
}

/// A cleanup-debt fixture: a plausible per-boundary estimate touching every
/// store.
pub fn cleanup_debt_estimate() -> PruneEstimate {
    PruneEstimate {
        vector_points: 120,
        artifacts: 4,
        graph_nodes: 8,
        graph_edges: 6,
        memory_records: 2,
        ledger_generations: 1,
        jobs: 0,
        cache_entries: 0,
    }
}

/// An old-generation prune fixture: only vector + ledger touched.
pub fn old_generation_estimate() -> PruneEstimate {
    PruneEstimate {
        vector_points: 512,
        ledger_generations: 1,
        ..PruneEstimate::default()
    }
}

/// A vector-orphan fixture: only vector points are dangling.
pub fn vector_orphan_estimate() -> PruneEstimate {
    PruneEstimate {
        vector_points: 37,
        ..PruneEstimate::default()
    }
}
