use super::*;

#[test]
fn legacy_job_heartbeat_preserves_job_attempt_and_event_sequence() {
    let id = uuid::Uuid::new_v4();
    let heartbeat = legacy_job_heartbeat(
        id,
        JobKind::Embed,
        3,
        Some("worker-1".to_string()),
        Some(17),
    );

    assert_eq!(heartbeat.job_id, JobId::new(id));
    assert_eq!(heartbeat.attempt, 3);
    assert_eq!(heartbeat.worker_id.as_deref(), Some("worker-1"));
    assert_eq!(heartbeat.phase, PipelinePhase::Embedding);
    assert_eq!(heartbeat.status, LifecycleStatus::Running);
    assert_eq!(heartbeat.last_event_sequence, Some(17));
}
