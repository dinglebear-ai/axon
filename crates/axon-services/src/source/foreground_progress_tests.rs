use super::*;
use axon_api::source::{
    JobId, JobStatusUpdate, LifecycleStatus, PipelinePhase, Severity, SourceProgressEvent,
    StageCounts,
};
use uuid::Uuid;

fn test_event(event_id: &str) -> SourceProgressEvent {
    let mut event = SourceProgressEvent::minimal(
        JobId::new(Uuid::from_u128(1)),
        0,
        PipelinePhase::Embedding,
        LifecycleStatus::Running,
        Severity::Info,
        "embedding chunks",
    );
    event.event_id = event_id.to_string();
    event
}

fn update(done: u64) -> JobStatusUpdate {
    JobStatusUpdate {
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: Some(10),
            chunks_done: done,
            bytes_total: None,
            bytes_done: 0,
        }),
        current: None,
        message: Some("embedding chunks".into()),
        error: None,
    }
}

#[tokio::test]
async fn snapshot_lane_keeps_only_the_latest_value() {
    let (tx, mut rx) = foreground_progress_channel_with_capacity(2);
    tx.snapshot(update(1));
    tx.snapshot(update(7));
    rx.snapshots.changed().await.unwrap();
    assert_eq!(
        rx.snapshots
            .borrow()
            .as_ref()
            .unwrap()
            .status()
            .unwrap()
            .counts
            .as_ref()
            .unwrap()
            .chunks_done,
        7,
    );
}

#[tokio::test]
async fn full_event_lane_sets_overflow_without_blocking() {
    let (tx, rx) = foreground_progress_channel_with_capacity(1);
    assert!(tx.event(test_event("evt_1")));
    assert!(!tx.event(test_event("evt_2")));
    assert!(rx.overflowed());
}

#[test]
fn taking_overflow_flag_is_edge_triggered() {
    let (tx, rx) = foreground_progress_channel_with_capacity(1);
    assert!(tx.event(test_event("evt_1")));
    assert!(!tx.event(test_event("evt_2")));
    assert!(rx.take_overflowed());
    assert!(!rx.take_overflowed());
    rx.mark_overflowed();
    assert!(rx.take_overflowed());
}
