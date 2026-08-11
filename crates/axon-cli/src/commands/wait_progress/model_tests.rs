use std::time::Duration;

use axon_api::source::*;

use super::*;

fn empty_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}

fn redaction_event(event_id: &str, chunk_id: &str) -> SourceProgressEvent {
    let job_id = JobId::new(uuid::Uuid::from_u128(7));
    let mut event = SourceProgressEvent::minimal(
        job_id,
        0,
        PipelinePhase::Preparing,
        LifecycleStatus::CompletedDegraded,
        Severity::Degraded,
        "secret-redaction-forbidden payload value",
    );
    event.event_id = event_id.to_string();
    event.current = Some(ProgressCurrent {
        source_item_key: None,
        document_id: None,
        chunk_id: Some(ChunkId::new(chunk_id)),
        adapter: Some("web".into()),
        provider: None,
        message: None,
    });
    event.warning = Some(SourceWarning {
        code: "secret_redaction_forbidden".into(),
        severity: Severity::Degraded,
        message: "secret-redaction-forbidden payload value".into(),
        source_item_key: None,
        retryable: false,
    });
    event
}

fn embedding_update(done: u64, total: u64) -> JobStatusUpdate {
    let mut counts = empty_counts();
    counts.chunks_total = Some(total);
    counts.chunks_done = done;
    JobStatusUpdate {
        job_id: JobId::new(uuid::Uuid::from_u128(7)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(counts),
        current: None,
        message: Some("embedding chunks".into()),
        error: None,
    }
}

#[test]
fn embedding_family_collapses_to_one_operator_phase() {
    assert_eq!(
        operator_phase(PipelinePhase::Batching),
        OperatorPhase::Embed
    );
    assert_eq!(
        operator_phase(PipelinePhase::Embedding),
        OperatorPhase::Embed
    );
    assert_eq!(
        operator_phase(PipelinePhase::Vectorizing),
        OperatorPhase::Embed
    );
    assert_eq!(
        operator_phase(PipelinePhase::Upserting),
        OperatorPhase::Publish
    );
}

#[test]
fn repeated_redaction_holds_become_one_neutral_notice() {
    let mut model = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    model.apply_event(redaction_event("evt_1", "chunk_1"));
    model.apply_event(redaction_event("evt_2", "chunk_2"));
    assert_eq!(model.notices.len(), 1);
    assert_eq!(model.notices[0].count, 2);
    assert_eq!(model.notices[0].message, "secret policy held 2 chunks");
    assert!(!model.notices[0].message.contains("chunk_1"));
}

#[test]
fn duplicate_event_id_is_ignored() {
    let mut model = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    let event = redaction_event("evt_1", "chunk_1");
    assert!(model.apply_event(event.clone()));
    assert!(!model.apply_event(event));
    assert_eq!(model.notices[0].count, 1);
}

#[test]
fn identical_snapshot_does_not_mark_the_model_dirty_twice() {
    let mut model = WaitViewModel::source("file:///repo", Some(SourceScope::Site));
    let update = embedding_update(3, 10);
    assert!(model.apply_snapshot(update.clone()));
    assert!(!model.apply_snapshot(update));
}

#[test]
fn subsecond_unremarkable_phase_does_not_leave_a_milestone() {
    let mut model = WaitViewModel::source("https://example.com", Some(SourceScope::Page));
    model.start_phase_at(PipelinePhase::Resolving, Duration::ZERO);
    model.complete_phase_at(PipelinePhase::Resolving, Duration::from_millis(200));
    assert!(model.milestones.is_empty());
}

#[test]
fn batch_uses_one_most_recent_active_target() {
    let mut batch = BatchWaitViewModel::new(3);
    batch.running(0, "a");
    batch.running(1, "b");
    batch.completed(0);
    assert_eq!(batch.summary(), "1/3 complete · 1 active · 1 queued");
    assert_eq!(
        batch.active_detail().map(|target| target.target.as_str()),
        Some("b")
    );
}
