use super::*;
use axon_api::source::prune::PruneTargetKind;

#[test]
fn debt_order_is_the_contract_order() {
    let order = debt_execution_order();
    assert_eq!(
        order,
        [
            PruneTargetKind::Vector,
            PruneTargetKind::Artifact,
            PruneTargetKind::Graph,
            PruneTargetKind::Memory,
            PruneTargetKind::Ledger,
            PruneTargetKind::JobRetention,
            PruneTargetKind::Cache,
        ]
    );
}

#[test]
fn ordering_is_idempotent() {
    let mut targets = vec![
        PruneTargetKind::Ledger,
        PruneTargetKind::Vector,
        PruneTargetKind::Graph,
    ];
    order_debt_targets(&mut targets);
    let once = targets.clone();
    order_debt_targets(&mut targets);
    assert_eq!(once, targets);
    assert_eq!(
        targets,
        vec![
            PruneTargetKind::Vector,
            PruneTargetKind::Graph,
            PruneTargetKind::Ledger,
        ]
    );
}

#[test]
fn ledger_runs_last_after_ordering() {
    let mut targets = vec![
        PruneTargetKind::Ledger,
        PruneTargetKind::Vector,
        PruneTargetKind::Artifact,
    ];
    assert!(!ledger_runs_last(&targets));
    order_debt_targets(&mut targets);
    assert!(ledger_runs_last(&targets));
}

#[test]
fn ledger_runs_last_trivially_when_absent() {
    let targets = vec![PruneTargetKind::Vector, PruneTargetKind::Cache];
    assert!(ledger_runs_last(&targets));
}
