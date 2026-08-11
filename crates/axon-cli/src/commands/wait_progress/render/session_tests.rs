use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent, JobKind, JobPriority,
    LifecycleStatus, MetadataMap, PipelinePhase, Severity, SourceProgressEvent, SourceScope,
    SourceWarning, StageCounts, Timestamp, Visibility,
};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_services::source::foreground_progress::foreground_progress_channel;

use super::*;

#[tokio::test]
async fn overflow_reconciles_public_events_once_by_event_id() {
    let store = Arc::new(FakeJobWatchStore::new());
    let job = JobStore::create(store.as_ref(), job_create())
        .await
        .unwrap();
    let event = warning_event(job.job_id);
    JobStore::append_event(store.as_ref(), event.clone())
        .await
        .unwrap();

    let (sender, receiver) = foreground_progress_channel();
    sender.job_started(job.job_id);
    receiver.mark_overflowed();
    let cfg = Config {
        quiet: true,
        ..Config::default()
    };
    let mut session = WaitProgressSession::source(
        &cfg,
        "https://example.com",
        Some(SourceScope::Site),
        receiver,
        Some(store),
    );
    session.apply_latest_snapshot();
    session.reconcile_if_overflowed().await;

    assert_eq!(session.model.notices.len(), 1);
    assert_eq!(session.model.notices[0].count, 1);
    session.apply_event(event);
    assert_eq!(session.model.notices[0].count, 1);
    assert_eq!(session.last_durable_sequence, Some(1));
}

fn warning_event(job_id: JobId) -> SourceProgressEvent {
    SourceProgressEvent {
        event_id: "evt_warning_1".into(),
        sequence: 1,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Preparing,
        status: LifecycleStatus::Running,
        severity: Severity::Degraded,
        visibility: Visibility::Public,
        message: "secret policy held a chunk".into(),
        timestamp: Timestamp("2026-08-11T18:03:24Z".into()),
        source_id: None,
        canonical_uri: Some("https://example.com".into()),
        adapter: None,
        scope: Some(SourceScope::Site),
        generation: None,
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        timing: None,
        current: None,
        throughput: None,
        retry: None,
        warning: Some(SourceWarning {
            code: "secret_redaction_forbidden".into(),
            severity: Severity::Degraded,
            message: "secret policy held a chunk".into(),
            source_item_key: None,
            retryable: false,
        }),
        error: None,
    }
}

fn job_create() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_wait_progress".into()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: None,
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_wait_progress")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".into()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}
