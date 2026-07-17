use axon_api::job_progress::JobPhase;

#[test]
fn job_phase_terminal_classification_is_stable() {
    // Guards the contract the palette poll loop relies on to stop polling.
    assert!(JobPhase::Done.is_terminal());
    assert!(JobPhase::Failed.is_terminal());
    assert!(JobPhase::Canceled.is_terminal());
    assert!(!JobPhase::Running.is_terminal());
    assert!(!JobPhase::Pending.is_terminal());
}
