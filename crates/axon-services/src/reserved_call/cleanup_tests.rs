use super::*;

#[test]
fn cleanup_drain_dependencies_have_one_shared_context() {
    assert!(std::mem::size_of::<CleanupDrainContext<'static>>() > 0);
}

#[test]
fn cleanup_reservations_use_background_cleaning_context_with_unique_fences() {
    let job_id = JobId::new(uuid::Uuid::from_u128(42));
    let first = cleanup_context(job_id, "vector-delete");
    let second = cleanup_context(job_id, "vector-delete");

    assert_eq!(first.phase, Some(PipelinePhase::Cleaning));
    assert_eq!(first.priority, JobPriority::Background);
    assert_eq!(first.attempt, 0);
    assert_ne!(first.operation_id, second.operation_id);
    assert!(
        first
            .operation_id
            .starts_with("cleanup-debt:vector-delete:")
    );
}

#[test]
fn registry_construction_failure_cannot_report_zero_for_adapter_debt() {
    let affected = count_adapter_release_debt([
        CleanupDebtKind::VectorDelete,
        CleanupDebtKind::AdapterRelease,
        CleanupDebtKind::AdapterRelease,
    ]);

    let mut summary = DebtDrainSummary::default();
    mark_registry_failure(&mut summary, affected);

    assert_eq!(summary.failed, 2);
    assert_ne!(summary, DebtDrainSummary::default());
}
