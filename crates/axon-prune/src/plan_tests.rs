use super::*;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneEstimate, PruneSelector, PruneTargetKind};
use axon_api::source::vector::VectorDeleteSelector;

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
fn collection_selector_produces_vector_step_with_collection_delete_selector() {
    let sel = PruneSelector::Collection {
        collection: "axon".to_string(),
    };
    // A real, non-fabricated point count (the shape `estimate_collection_points`
    // in `axon-services` would feed in from the live vector store).
    let est = PruneEstimate {
        vector_points: 42,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est));
    let plan = planner.resolve(&sel);

    assert_eq!(plan.steps.len(), 1);
    let step = &plan.steps[0];
    assert_eq!(step.target, PruneTargetKind::Vector);
    assert_eq!(step.estimated_deletes, 42);
    assert_eq!(
        step.vector_selector,
        Some(VectorDeleteSelector::Collection {
            collection: "axon".to_string(),
        })
    );
    // Collection prunes name no source/generation identity.
    assert_eq!(step.source_id, None);
    assert_eq!(step.generation, None);
}

#[test]
fn vector_selector_uses_configured_collection_not_hardcoded_axon() {
    // D4 remediation (PR #418 review): a deployment with `AXON_COLLECTION`
    // set to something other than "axon" must not have its Source/Generation
    // prune steps hardcode "axon" as the delete target — that silently
    // deletes from the wrong collection (or a nonexistent one).
    let est = PruneEstimate {
        vector_points: 7,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est)).with_collection("cortex_v2");
    let plan = planner.resolve(&source_sel());

    let vector_step = plan
        .steps
        .iter()
        .find(|s| s.target == PruneTargetKind::Vector)
        .expect("vector step present");
    match vector_step.vector_selector.as_ref() {
        Some(VectorDeleteSelector::Source { collection, .. }) => {
            assert_eq!(collection, "cortex_v2");
        }
        other => panic!("expected Source selector, got {other:?}"),
    }
}

#[test]
fn generation_selector_also_uses_configured_collection() {
    let sel = PruneSelector::Generation {
        source_id: SourceId::new("owner/repo"),
        generation: SourceGenerationId::new("gen-2"),
    };
    let est = PruneEstimate {
        vector_points: 5,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est)).with_collection("cortex_v2");
    let plan = planner.resolve(&sel);

    let vector_step = plan
        .steps
        .iter()
        .find(|s| s.target == PruneTargetKind::Vector)
        .expect("vector step present");
    match vector_step.vector_selector.as_ref() {
        Some(VectorDeleteSelector::Generation { collection, .. }) => {
            assert_eq!(collection, "cortex_v2");
        }
        other => panic!("expected Generation selector, got {other:?}"),
    }
}

#[test]
fn collection_selector_keeps_its_own_named_collection_even_with_different_active_collection() {
    // A `Collection`-selector prune is allowed to name an arbitrary
    // collection distinct from the process's active `AXON_COLLECTION` (e.g.
    // cleaning up a stale collection) — `with_collection` must not clobber
    // that explicit name.
    let sel = PruneSelector::Collection {
        collection: "some_other_collection".to_string(),
    };
    let est = PruneEstimate {
        vector_points: 3,
        ..PruneEstimate::default()
    };
    let planner = PrunePlanner::new(FakeScopeSource::new(est)).with_collection("cortex_v2");
    let plan = planner.resolve(&sel);

    assert_eq!(
        plan.steps[0].vector_selector,
        Some(VectorDeleteSelector::Collection {
            collection: "some_other_collection".to_string(),
        })
    );
}

#[test]
fn empty_estimate_yields_empty_plan() {
    let planner = PrunePlanner::new(FakeScopeSource::new(PruneEstimate::default()));
    let plan = planner.resolve(&source_sel());
    assert!(plan.steps.is_empty());
    assert_eq!(plan.estimated, PruneEstimate::default());
}
