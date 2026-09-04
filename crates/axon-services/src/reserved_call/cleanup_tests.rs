use super::*;

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
