use super::*;
use axon_api::source::enums::LifecycleStatus;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneStepResult, PruneTargetKind};

fn step(target: PruneTargetKind, deleted: u64, status: LifecycleStatus) -> PruneStepResult {
    PruneStepResult {
        target,
        status,
        deleted,
        skipped_reason: None,
        source_id: Some(SourceId::new("owner/repo")),
        generation: Some(SourceGenerationId::new("gen-1")),
    }
}

#[test]
fn aggregates_per_boundary_counts() {
    let steps = vec![
        step(PruneTargetKind::Vector, 100, LifecycleStatus::Completed),
        step(PruneTargetKind::Artifact, 3, LifecycleStatus::Completed),
        step(PruneTargetKind::Graph, 8, LifecycleStatus::Completed),
        step(PruneTargetKind::Ledger, 1, LifecycleStatus::Completed),
    ];
    let counts = counts_from_steps(&steps);
    assert_eq!(counts.vector_points, 100);
    assert_eq!(counts.artifacts, 3);
    assert_eq!(counts.graph_nodes, 8);
    assert_eq!(counts.ledger_generations, 1);
    assert_eq!(counts.total(), 112);
}

#[test]
fn skipped_steps_contribute_zero() {
    let mut skipped = step(PruneTargetKind::Vector, 0, LifecycleStatus::Skipped);
    skipped.skipped_reason = Some("already drained".into());
    let steps = vec![
        skipped,
        step(PruneTargetKind::Cache, 5, LifecycleStatus::Completed),
    ];
    let counts = counts_from_steps(&steps);
    assert_eq!(counts.vector_points, 0);
    assert_eq!(counts.cache_entries, 5);
    assert_eq!(counts.total(), 5);
}

#[test]
fn same_boundary_twice_sums() {
    let steps = vec![
        step(PruneTargetKind::Vector, 10, LifecycleStatus::Completed),
        step(PruneTargetKind::Vector, 15, LifecycleStatus::Completed),
    ];
    assert_eq!(counts_from_steps(&steps).vector_points, 25);
}
