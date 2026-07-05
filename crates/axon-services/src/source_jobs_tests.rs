use axon_api::source::*;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

use super::*;

fn request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_docs".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: Some(SourceId::new("src_docs")),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: Some(serde_json::json!({"source": "https://example.com/docs"})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn target_source_job_services_delegate_to_job_store() {
    let store = FakeJobWatchStore::new();
    let job = create_job(&store, request()).await.expect("create");
    assert_eq!(job.kind, JobKind::Source);

    JobStore::update_status(
        &store,
        JobStatusUpdate {
            source_id: None,
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        },
    )
    .await
    .expect("running");
    JobStore::append_event(
        &store,
        SourceProgressEvent {
            event_id: "evt_1".to_string(),
            sequence: 1,
            job_id: job.job_id,
            attempt: 1,
            stage_id: None,
            batch_id: None,
            reservation_id: None,
            checkpoint_id: None,
            dedupe_key: None,
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            severity: Severity::Info,
            visibility: Visibility::Public,
            message: "embedding".to_string(),
            timestamp: Timestamp("2026-07-01T00:00:01Z".to_string()),
            source_id: Some(SourceId::new("src_docs")),
            canonical_uri: Some("https://example.com/docs".to_string()),
            adapter: None,
            scope: None,
            generation: None,
            counts: empty_counts(),
            timing: None,
            current: None,
            throughput: None,
            retry: None,
            warning: None,
            error: None,
        },
    )
    .await
    .expect("event");

    let status = job_status(&store, job.job_id)
        .await
        .expect("status")
        .expect("job status");
    assert_eq!(status.summary.status, LifecycleStatus::Running);
    assert_eq!(status.latest_event_sequence, Some(1));

    let jobs = list_jobs(
        &store,
        JobListRequest {
            status: Some(LifecycleStatus::Running),
            kind: Some(JobKind::Source),
            source_id: Some(SourceId::new("src_docs")),
            watch_id: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .expect("list jobs");
    assert_eq!(jobs.items.len(), 1);

    let events = list_events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            after_sequence: None,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .expect("list events");
    assert_eq!(events.events.len(), 1);

    let cancel = cancel_job(
        &store,
        job.job_id,
        JobCancelRequest {
            reason: Some("user".to_string()),
            force_after_ms: None,
        },
    )
    .await
    .expect("cancel");
    assert_eq!(cancel.status, LifecycleStatus::Canceling);

    JobStore::update_status(
        &store,
        JobStatusUpdate {
            source_id: None,
            job_id: job.job_id,
            status: LifecycleStatus::Canceled,
            phase: PipelinePhase::Canceled,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        },
    )
    .await
    .expect("canceled");
    let retry = retry_job(
        &store,
        job.job_id,
        JobRetryRequest {
            mode: JobRetryMode::SameConfig,
            from_phase: None,
            idempotency_key: None,
            overrides: MetadataMap::new(),
        },
    )
    .await
    .expect("retry");
    assert_eq!(retry.original_job_id, job.job_id);

    let artifacts = list_artifacts(
        &store,
        JobArtifactListRequest {
            job_id: retry.retry_job.job_id,
            kind: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .expect("artifacts");
    assert!(artifacts.artifacts.is_empty());

    let recovery = recover_jobs(
        &store,
        JobRecoveryRequest {
            kind: Some(JobKind::Source),
            stale_before: None,
            limit: Some(10),
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: true,
        },
    )
    .await
    .expect("recover");
    assert_eq!(recovery.jobs_scanned, 0);

    let cleanup = cleanup_jobs(
        &store,
        JobCleanupRequest {
            dry_run: true,
            kind: Some(JobKind::Source),
            older_than: None,
            status: None,
            limit: Some(10),
            older_than_seconds: None,
            confirm_all_terminal: true,
        },
    )
    .await
    .expect("cleanup");
    assert_eq!(cleanup.jobs_pruned, 0);
}

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
