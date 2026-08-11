use super::*;

#[test]
fn mixed_batch_failure_never_renders_a_success_terminal() {
    let (_forwarder, updates) = batch_progress_channel();
    let cfg = Config {
        quiet: true,
        ..Config::default()
    };
    let mut session = BatchProgressSession::new(&cfg, 2, updates);
    session.apply(BatchProgressUpdate::Started {
        index: 0,
        target: "ok".to_string(),
    });
    session.apply(BatchProgressUpdate::Finished {
        index: 0,
        outcome: BatchTerminalOutcome::Completed,
    });
    session.apply(BatchProgressUpdate::Started {
        index: 1,
        target: "failed".to_string(),
    });
    session.apply(BatchProgressUpdate::Finished {
        index: 1,
        outcome: BatchTerminalOutcome::Failed,
    });

    let terminal = session.formatted(true).terminal.expect("terminal");
    assert!(terminal.starts_with('⚠'));
    assert!(!terminal.starts_with('✓'));
    assert!(terminal.contains("1 failed"));
}

#[test]
fn batch_forwarding_keeps_routed_kind_when_status_is_newer() {
    use axon_api::source::{
        JobId, JobStatusUpdate, LifecycleStatus, PipelinePhase, SourceKind, StageCounts,
    };
    use axon_services::source::foreground_progress::foreground_progress_channel;

    let (sender, mut receiver) = foreground_progress_channel();
    let job_id = JobId::new(uuid::Uuid::from_u128(9));
    sender.routed(job_id, SourceKind::Web);
    sender.snapshot(JobStatusUpdate {
        job_id,
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Fetching,
        stage_id: None,
        counts: Some(StageCounts {
            items_total: Some(4),
            items_done: 2,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        }),
        current: None,
        message: None,
        error: None,
    });
    let (updates, mut received) = mpsc::unbounded_channel();

    forward_snapshot(0, &mut receiver, &updates);

    assert!(matches!(
        received.try_recv().unwrap(),
        BatchProgressUpdate::Routed {
            source_kind: SourceKind::Web,
            ..
        }
    ));
    assert!(matches!(
        received.try_recv().unwrap(),
        BatchProgressUpdate::Snapshot { .. }
    ));
}
