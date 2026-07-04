use super::*;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneEstimate, PruneSelector, PruneTargetKind};

use crate::executor::steps_in_execution_order;
use crate::testing::{FakeScopeSource, cleanup_debt_estimate};

fn source_sel() -> PruneSelector {
    PruneSelector::Source {
        source_id: SourceId::new("owner/repo"),
    }
}

#[test]
fn resolves_selector_into_ordered_plan() {
    let planner = PrunePlanner::new(FakeScopeSource::new(cleanup_debt_estimate()));
    let plan = planner.resolve(&source_sel());

    assert_eq!(plan.selector, source_sel());
    assert!(plan.destructive);
    assert!(plan.requires_admin);
    // Steps must be in cleanup-debt execution order.
    assert!(steps_in_execution_order(&plan.steps));
    // First present boundary is vector, last is ledger (no jobs/cache here).
    assert_eq!(plan.steps.first().unwrap().target, PruneTargetKind::Vector);
    assert_eq!(plan.steps.last().unwrap().target, PruneTargetKind::Ledger);
}

#[test]
fn only_positive_boundaries_produce_steps() {
    // Only vector points; every other boundary is zero.
    let est = PruneEstimate {
        vector_points: 10,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est));
    let plan = planner.resolve(&source_sel());

    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].target, PruneTargetKind::Vector);
    assert_eq!(plan.steps[0].estimated_deletes, 10);
}

#[test]
fn graph_step_sums_nodes_and_edges() {
    let est = PruneEstimate {
        graph_nodes: 3,
        graph_edges: 5,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est));
    let plan = planner.resolve(&source_sel());
    let graph = plan
        .steps
        .iter()
        .find(|s| s.target == PruneTargetKind::Graph)
        .expect("graph step present");
    assert_eq!(graph.estimated_deletes, 8);
}

#[test]
fn generation_selector_stamps_source_and_generation_on_steps() {
    let sel = PruneSelector::Generation {
        source_id: SourceId::new("owner/repo"),
        generation: SourceGenerationId::new("gen-2"),
    };
    let est = PruneEstimate {
        vector_points: 5,
        ledger_generations: 1,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est));
    let plan = planner.resolve(&sel);

    for step in &plan.steps {
        assert_eq!(step.source_id, Some(SourceId::new("owner/repo")));
        assert_eq!(step.generation, Some(SourceGenerationId::new("gen-2")));
    }
}

#[test]
fn empty_estimate_yields_empty_plan() {
    let planner = PrunePlanner::new(FakeScopeSource::new(PruneEstimate::default()));
    let plan = planner.resolve(&source_sel());
    assert!(plan.steps.is_empty());
    assert_eq!(plan.estimated, PruneEstimate::default());
}
