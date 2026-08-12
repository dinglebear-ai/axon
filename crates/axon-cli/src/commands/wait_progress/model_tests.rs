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
fn aggregated_redaction_warning_preserves_skipped_chunk_count() {
    let mut model = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    let mut event = redaction_event("evt_1", "chunk_1");
    event.warning = Some(SourceWarning {
        code: "source.vectorize.redaction_skipped_chunks".into(),
        severity: Severity::Warning,
        message: "skipped 7 chunk(s) with secret-redaction-forbidden payload values".into(),
        source_item_key: None,
        retryable: false,
    });

    model.apply_event(event);

    assert_eq!(model.notices[0].count, 7);
    assert_eq!(model.notices[0].message, "secret policy held 7 chunks");
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
fn early_failure_without_progress_still_has_a_terminal() {
    let mut model = WaitViewModel::source("bad source", None);

    assert!(model.finish(LifecycleStatus::Failed));
    let terminal = model.terminal.expect("failed terminal");
    assert_eq!(terminal.status, TerminalStatus::Failed);
    assert_eq!(terminal.summary, "source did not start");
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
    batch.finish(0, BatchTerminalOutcome::Completed);
    assert_eq!(batch.summary(), "1/3 complete · 1 active · 1 queued");
    assert_eq!(
        batch.active_detail().map(|target| target.target.as_str()),
        Some("b")
    );
}

#[test]
fn phase_completion_event_does_not_finish_the_job() {
    let mut model = WaitViewModel::source("https://example.com", Some(SourceScope::Site));
    let event = SourceProgressEvent::minimal(
        JobId::new(uuid::Uuid::from_u128(7)),
        0,
        PipelinePhase::Fetching,
        LifecycleStatus::Completed,
        Severity::Info,
        "fetch complete",
    );
    model.apply_event(event);
    assert!(model.terminal.is_none());
}

#[test]
fn routed_source_kind_controls_acquisition_units() {
    let mut model = WaitViewModel::source("https://example.com", Some(SourceScope::Site));
    model.set_source_kind(SourceKind::Web);
    let mut counts = empty_counts();
    counts.items_total = Some(4);
    counts.items_done = 2;
    model.apply_snapshot(JobStatusUpdate {
        job_id: JobId::new(uuid::Uuid::from_u128(7)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Fetching,
        stage_id: None,
        counts: Some(counts),
        current: None,
        message: None,
        error: None,
    });
    assert_eq!(model.active.expect("active").unit, "pages");
}

#[test]
fn extract_progress_reports_completed_urls_and_cumulative_items() {
    let mut model = WaitViewModel::source("2 URLs", None);
    let progress = ExtractProgress::new(2).completed_url("https://example.com/a", 4);
    assert!(model.apply_extract_progress(&progress));
    let active = model.active.expect("active extract progress");
    assert_eq!(active.phase, OperatorPhase::Extract);
    assert_eq!(
        (active.done, active.total, active.unit),
        (1, Some(2), "URLs")
    );
    assert_eq!(
        active.current.as_deref(),
        Some("last completed: https://example.com/a · 4 items")
    );
}
