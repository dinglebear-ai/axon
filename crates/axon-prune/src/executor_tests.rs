use super::*;
use axon_api::source::enums::LifecycleStatus;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneSelector, PruneTargetKind};

use crate::plan::PrunePlanner;
use crate::testing::{FakePruneTarget, FakeScopeSource, cleanup_debt_estimate};

fn source_sel() -> PruneSelector {
    PruneSelector::Source {
        source_id: SourceId::new("owner/repo"),
    }
}

fn cleanup_plan() -> axon_api::source::prune::PrunePlan {
    PrunePlanner::new(FakeScopeSource::new(cleanup_debt_estimate())).resolve(&source_sel())
}

#[tokio::test]
async fn dry_run_planning_leaves_target_untouched() {
    // Building a plan must not delete anything. We prove this by seeding a
    // fresh target from the plan and confirming nothing was applied yet.
    let plan = cleanup_plan();
    let target = FakePruneTarget::from_steps(&plan.steps);
    assert!(target.applied_log().is_empty());
    assert_eq!(
        target.remaining_for(PruneTargetKind::Vector),
        cleanup_debt_estimate().vector_points
    );
}

#[tokio::test]
async fn executes_steps_in_cleanup_debt_order() {
    let plan = cleanup_plan();
    let target = FakePruneTarget::from_steps(&plan.steps);
    let executor = PruneExecutor::new(target);
    let result = executor.execute(&plan).await.unwrap();

    // Result step order must be the canonical execution order.
    let order: Vec<PruneTargetKind> = result.steps.iter().map(|s| s.target).collect();
    let ranks: Vec<usize> = order.iter().map(|t| t.order_rank()).collect();
    assert!(
        ranks.windows(2).all(|w| w[0] <= w[1]),
        "not ordered: {order:?}"
    );
    // Ledger is after vector/artifact/graph/memory.
    let ledger_pos = order
        .iter()
        .position(|t| *t == PruneTargetKind::Ledger)
        .unwrap();
    let vector_pos = order
        .iter()
        .position(|t| *t == PruneTargetKind::Vector)
        .unwrap();
    assert!(vector_pos < ledger_pos);
}

#[tokio::test]
async fn deleted_counts_match_estimate_on_full_execute() {
    let plan = cleanup_plan();
    let est = cleanup_debt_estimate();
    let target = FakePruneTarget::from_steps(&plan.steps);
    let executor = PruneExecutor::new(target);
    let result = executor.execute(&plan).await.unwrap();

    assert_eq!(result.status, LifecycleStatus::Completed);
    assert_eq!(result.deleted_counts.vector_points, est.vector_points);
    assert_eq!(result.deleted_counts.artifacts, est.artifacts);
    // graph_nodes tally holds the combined graph step delete.
    assert_eq!(
        result.deleted_counts.graph_nodes,
        est.graph_nodes + est.graph_edges
    );
    assert_eq!(result.deleted_counts.memory_records, est.memory_records);
    assert_eq!(
        result.deleted_counts.ledger_generations,
        est.ledger_generations
    );
    assert_eq!(result.cleanup_debt_remaining, 0);
}

#[tokio::test]
async fn re_execution_is_idempotent() {
    let plan = cleanup_plan();
    let target = FakePruneTarget::from_steps(&plan.steps);
    let executor = PruneExecutor::new(target);

    let first = executor.execute(&plan).await.unwrap();
    assert!(first.deleted_counts.total() > 0);

    // Second run against the drained target deletes nothing and skips.
    let second = executor.execute(&plan).await.unwrap();
    assert_eq!(second.deleted_counts.total(), 0);
    assert!(
        second
            .steps
            .iter()
            .all(|s| s.status == LifecycleStatus::Skipped)
    );
    assert_eq!(second.cleanup_debt_remaining, 0);
}

#[tokio::test]
async fn generation_fence_blocks_current_generation() {
    let sel = PruneSelector::Generation {
        source_id: SourceId::new("owner/repo"),
        generation: SourceGenerationId::new("gen-current"),
    };
    let est = axon_api::source::prune::PruneEstimate {
        vector_points: 3,
        ..Default::default()
    };
    let plan = PrunePlanner::new(FakeScopeSource::new(est)).resolve(&sel);
    // Target reports the same generation as current -> fenced.
    let target = FakePruneTarget::from_steps(&plan.steps)
        .with_current_generation(SourceGenerationId::new("gen-current"));
    let executor = PruneExecutor::new(target);

    let out = executor.execute(&plan).await;
    assert!(matches!(
        out,
        Err(crate::safety::PruneDenied::CurrentGenerationFenced { .. })
    ));
}

#[tokio::test]
async fn old_generation_passes_fence_and_deletes() {
    let sel = PruneSelector::Generation {
        source_id: SourceId::new("owner/repo"),
        generation: SourceGenerationId::new("gen-old"),
    };
    let est = axon_api::source::prune::PruneEstimate {
        vector_points: 3,
        ..Default::default()
    };
    let plan = PrunePlanner::new(FakeScopeSource::new(est)).resolve(&sel);
    let target = FakePruneTarget::from_steps(&plan.steps)
        .with_current_generation(SourceGenerationId::new("gen-current"));
    let executor = PruneExecutor::new(target);

    let out = executor.execute(&plan).await.unwrap();
    assert_eq!(out.deleted_counts.vector_points, 3);
}

#[tokio::test]
async fn partial_failure_records_remaining_debt() {
    let plan = cleanup_plan();
    // Force the graph boundary to fail; other boundaries still delete.
    let target = FakePruneTarget::from_steps(&plan.steps).failing(PruneTargetKind::Graph);
    let executor = PruneExecutor::new(target);
    let result = executor.execute(&plan).await.unwrap();

    assert_eq!(result.status, LifecycleStatus::CompletedDegraded);
    // The failed graph step contributes its estimated deletes to remaining debt.
    let graph_est = cleanup_debt_estimate().graph_nodes + cleanup_debt_estimate().graph_edges;
    assert_eq!(result.cleanup_debt_remaining, graph_est);
    // Non-graph boundaries still deleted.
    assert!(result.deleted_counts.vector_points > 0);
    let graph_step = result
        .steps
        .iter()
        .find(|s| s.target == PruneTargetKind::Graph)
        .unwrap();
    assert_eq!(graph_step.status, LifecycleStatus::Failed);
    assert_eq!(graph_step.deleted, 0);
    assert!(graph_step.skipped_reason.is_some());
}
