use axon_api::source::*;

use super::*;

#[tokio::test]
async fn fake_job_store_tracks_status_events_and_heartbeats() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();

    JobStore::update_status(
        &store,
        JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            error: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        JobStore::get(&store, job.job_id)
            .await
            .unwrap()
            .unwrap()
            .phase,
        PipelinePhase::Embedding
    );

    assert_eq!(
        JobStore::capabilities(&store).await.unwrap().0.owner_crate,
        "axon-jobs"
    );
    JobStore::reset(&store).await.unwrap();
    assert!(JobStore::get(&store, job.job_id).await.unwrap().is_none());
}

#[tokio::test]
async fn fake_job_store_rejects_unknown_jobs_and_terminal_restarts() {
    let store = FakeJobWatchStore::new();
    let missing = JobId::new(Uuid::from_u128(999));

    let err = JobStore::heartbeat(
        &store,
        JobHeartbeat {
            job_id: missing,
            phase: PipelinePhase::Embedding,
            timestamp: Timestamp("2026-07-01T00:00:00Z".to_string()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.not_found");

    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    JobStore::update_status(
        &store,
        JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Complete,
            error: None,
        },
    )
    .await
    .unwrap();
    let err = JobStore::update_status(
        &store,
        JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            error: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.invalid_transition");
}

#[tokio::test]
async fn fake_job_store_filters_lists_and_rejects_cursors() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    let listed = JobStore::list(
        &store,
        JobListRequest {
            status: Some(LifecycleStatus::Queued),
            kind: Some(JobKind::Source),
            source_id: None,
            watch_id: None,
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(listed.total, Some(1));
    assert_eq!(listed.items[0].job_id, job.job_id);

    let err = JobStore::list(
        &store,
        JobListRequest {
            status: None,
            kind: None,
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: Some("next".to_string()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.cursor_unsupported");
}

#[tokio::test]
async fn fake_job_store_filters_events_and_resets_job_ids_only() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();

    for (sequence, phase, severity) in [
        (1, PipelinePhase::Embedding, Severity::Info),
        (2, PipelinePhase::Embedding, Severity::Warning),
        (3, PipelinePhase::Publishing, Severity::Info),
    ] {
        JobStore::append_event(
            &store,
            SourceProgressEvent {
                event_id: format!("evt_{sequence}"),
                sequence,
                job_id: job.job_id,
                phase,
                status: LifecycleStatus::Running,
                severity,
                visibility: Visibility::Internal,
                message: "progress".to_string(),
                timestamp: Timestamp(format!("2026-07-01T00:00:0{sequence}Z")),
                source_id: None,
                canonical_uri: None,
                adapter: None,
                scope: None,
                generation: None,
                counts: empty_counts(),
                current: None,
                throughput: None,
                retry: None,
                warning: None,
                error: None,
            },
        )
        .await
        .unwrap();
    }

    let events = JobStore::events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            phase: Some(PipelinePhase::Embedding),
            severity: Some(Severity::Warning),
            since_sequence: Some(1),
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(events.items.len(), 1);
    assert_eq!(events.items[0].sequence, 2);

    let watch = WatchStore::create(
        &store,
        WatchRequest {
            source: "file:///repo".to_string(),
            schedule: WatchSchedule {
                every_seconds: 60,
                cron: None,
                timezone: None,
            },
            embed: true,
            options: AdapterOptions::default(),
            scope: Some(SourceScope::Directory),
            collection: None,
            enabled: Some(true),
        },
    )
    .await
    .unwrap();
    WatchStore::record_run(&store, watch.watch_id.clone(), job.job_id)
        .await
        .unwrap();
    JobStore::reset(&store).await.unwrap();
    assert!(
        WatchStore::get(&store, watch.watch_id.clone())
            .await
            .unwrap()
            .is_some()
    );
    let next = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(next.job_id, JobId::new(Uuid::from_u128(1)));
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

#[tokio::test]
async fn fake_watch_store_creates_updates_lists_and_records_history() {
    let store = FakeJobWatchStore::new();
    let watch = WatchStore::create(
        &store,
        WatchRequest {
            source: "file:///repo".to_string(),
            schedule: WatchSchedule {
                every_seconds: 60,
                cron: None,
                timezone: None,
            },
            embed: true,
            options: AdapterOptions::default(),
            scope: Some(SourceScope::Directory),
            collection: None,
            enabled: Some(true),
        },
    )
    .await
    .unwrap();

    let updated = WatchStore::update(
        &store,
        watch.watch_id.clone(),
        WatchUpdateRequest {
            schedule: None,
            enabled: Some(false),
            embed: None,
            scope: Some(SourceScope::Repo),
            options: None,
        },
    )
    .await
    .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.scope, SourceScope::Repo);

    let listed = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: Some(false),
            source_id: Some(updated.source_id.clone()),
            adapter: Some("fake".to_string()),
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(listed.items.len(), 1);

    let job = JobStore::create(
        &store,
        JobCreateRequest {
            kind: JobKind::Source,
            request: SourceRequest::new("file:///repo"),
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        },
    )
    .await
    .unwrap();
    WatchStore::record_run(&store, watch.watch_id.clone(), job.job_id)
        .await
        .unwrap();
    let history = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: watch.watch_id,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(history.runs.len(), 1);
    let listed = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: Some(false),
            source_id: Some(updated.source_id),
            adapter: Some("fake".to_string()),
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(listed.items[0].last_job_id, Some(job.job_id));

    WatchStore::reset(&store).await.unwrap();
    let empty = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert!(empty.items.is_empty());
}

#[tokio::test]
async fn fake_watch_store_rejects_dangling_run_links() {
    let store = FakeJobWatchStore::new();
    let watch = WatchStore::create(
        &store,
        WatchRequest {
            source: "file:///repo".to_string(),
            schedule: WatchSchedule {
                every_seconds: 60,
                cron: None,
                timezone: None,
            },
            embed: true,
            options: AdapterOptions::default(),
            scope: Some(SourceScope::Directory),
            collection: None,
            enabled: Some(true),
        },
    )
    .await
    .unwrap();

    let err = WatchStore::record_run(
        &store,
        watch.watch_id.clone(),
        JobId::new(Uuid::from_u128(404)),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.not_found");

    let err = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: WatchId::new("missing"),
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "watch.not_found");
}
