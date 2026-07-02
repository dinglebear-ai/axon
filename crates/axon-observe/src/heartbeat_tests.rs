use axon_api::source::{JobId, LifecycleStatus, PipelinePhase, StageCounts};

use crate::heartbeat::JobHeartbeatExt;

#[test]
fn heartbeat_defaults_match_foreground_and_background_contract() {
    assert_eq!(crate::heartbeat::foreground_interval_secs(), 5);
    assert_eq!(crate::heartbeat::background_interval_secs(), 15);
}

#[test]
fn heartbeat_builder_carries_liveness_and_progress_context() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let heartbeat = crate::heartbeat::heartbeat(
        job_id,
        3,
        PipelinePhase::Embedding,
        LifecycleStatus::Waiting,
    )
    .with_worker("worker_1")
    .with_last_event_sequence(42)
    .with_counts(StageCounts {
        items_total: Some(10),
        items_done: 4,
        documents_total: None,
        documents_done: 0,
        chunks_total: Some(20),
        chunks_done: 8,
        bytes_total: None,
        bytes_done: 0,
    });

    assert_eq!(heartbeat.job_id, job_id);
    assert_eq!(heartbeat.attempt, 3);
    assert_eq!(heartbeat.status, LifecycleStatus::Waiting);
    assert_eq!(heartbeat.worker_id.as_deref(), Some("worker_1"));
    assert_eq!(heartbeat.last_event_sequence, Some(42));
    assert_eq!(heartbeat.counts.unwrap().chunks_done, 8);
}
