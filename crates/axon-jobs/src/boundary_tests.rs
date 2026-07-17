use axon_api::source::*;

use crate::boundary::WatchStore;

use super::*;

#[tokio::test]
async fn fake_job_store_tracks_status_events_and_heartbeats() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();

    JobStore::update_status(
        &store,
        JobStatusUpdate {
            source_id: Some(SourceId::new("src_docs")),
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
    .unwrap();
    assert_eq!(
        JobStore::get(&store, job.job_id)
            .await
            .unwrap()
            .unwrap()
            .source_id,
        Some(SourceId::new("src_docs"))
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
            attempt: 1,
            worker_id: None,
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
            sequence: 0,
            last_progress_at: None,
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.not_found");

    let job = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, job.job_id, LifecycleStatus::Running).await;
    JobStore::update_status(
        &store,
        JobStatusUpdate {
            source_id: None,
            job_id: job.job_id,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        },
    )
    .await
    .unwrap();
    let err = JobStore::update_status(
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
    .unwrap_err();
    assert_eq!(err.code.to_string(), "job.invalid_transition");
}

#[tokio::test]
async fn fake_job_store_allows_declared_state_machine_edges() {
    for (from, to) in [
        (LifecycleStatus::Queued, LifecycleStatus::Blocked),
        (LifecycleStatus::Queued, LifecycleStatus::Running),
        (LifecycleStatus::Queued, LifecycleStatus::Canceling),
        (LifecycleStatus::Queued, LifecycleStatus::Expired),
        (LifecycleStatus::Blocked, LifecycleStatus::Queued),
        (LifecycleStatus::Blocked, LifecycleStatus::Running),
        (LifecycleStatus::Blocked, LifecycleStatus::Canceling),
        (LifecycleStatus::Blocked, LifecycleStatus::Failed),
        (LifecycleStatus::Blocked, LifecycleStatus::Expired),
        (LifecycleStatus::Running, LifecycleStatus::Waiting),
        (LifecycleStatus::Waiting, LifecycleStatus::Running),
        (LifecycleStatus::Running, LifecycleStatus::Canceling),
        (LifecycleStatus::Canceling, LifecycleStatus::Canceled),
        (LifecycleStatus::Canceling, LifecycleStatus::Failed),
        (LifecycleStatus::Running, LifecycleStatus::Completed),
        (LifecycleStatus::Running, LifecycleStatus::CompletedDegraded),
        (LifecycleStatus::Running, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Canceling),
        (LifecycleStatus::Waiting, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Expired),
    ] {
        let store = FakeJobWatchStore::new();
        let job = JobStore::create(&store, job_create()).await.unwrap();
        if from != LifecycleStatus::Queued {
            drive_job_to(&store, job.job_id, from).await;
        }

        JobStore::update_status(&store, status_update(job.job_id, to))
            .await
            .unwrap_or_else(|err| panic!("{from:?} -> {to:?} should be allowed: {err:?}"));
    }
}

#[tokio::test]
async fn fake_job_store_rejects_illegal_state_machine_edges_without_mutating_status() {
    for (from, to) in [
        (LifecycleStatus::Queued, LifecycleStatus::Completed),
        (LifecycleStatus::Completed, LifecycleStatus::Failed),
        (LifecycleStatus::Canceled, LifecycleStatus::Running),
        (LifecycleStatus::Failed, LifecycleStatus::Completed),
        (LifecycleStatus::Running, LifecycleStatus::Queued),
    ] {
        let store = FakeJobWatchStore::new();
        let job = JobStore::create(&store, job_create()).await.unwrap();
        if from != LifecycleStatus::Queued {
            drive_job_to(&store, job.job_id, from).await;
        }

        let err = JobStore::update_status(&store, status_update(job.job_id, to))
            .await
            .unwrap_err();
        assert_eq!(err.code.to_string(), "job.invalid_transition");
        assert_eq!(
            JobStore::get(&store, job.job_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            from
        );
    }
}

#[tokio::test]
async fn fake_job_store_filters_and_cursor_paginates_lists() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    let other = JobStore::create(&store, job_create()).await.unwrap();
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
    assert_eq!(listed.total, Some(2));
    assert!(listed.next_cursor.is_some());
    let second = JobStore::list(
        &store,
        JobListRequest {
            status: Some(LifecycleStatus::Queued),
            kind: Some(JobKind::Source),
            source_id: None,
            watch_id: None,
            limit: Some(1),
            cursor: listed.next_cursor,
        },
    )
    .await
    .unwrap();
    assert_eq!(second.total, None);
    assert_eq!(second.next_cursor, None);
    let returned = [listed.items[0].job_id, second.items[0].job_id];
    assert!(returned.contains(&job.job_id));
    assert!(returned.contains(&other.job_id));

    let invalid = JobStore::list(
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
    assert_eq!(invalid.code.to_string(), "job.cursor_invalid");
}

#[tokio::test]
async fn fake_job_store_filters_events_and_resets_job_ids_only() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();

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
                attempt: 1,
                stage_id: None,
                batch_id: None,
                reservation_id: None,
                checkpoint_id: None,
                dedupe_key: None,
                phase,
                status: LifecycleStatus::Running,
                severity,
                visibility: Visibility::Redacted,
                message: "progress".to_string(),
                timestamp: Timestamp(format!("2026-07-01T00:00:0{sequence}Z")),
                source_id: None,
                canonical_uri: None,
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
        .unwrap();
    }

    let events = JobStore::events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            phase: Some(PipelinePhase::Embedding),
            severity: Some(Severity::Warning),
            visibility: Some(Visibility::Redacted),
            after_sequence: Some(1),
            since_sequence: Some(1),
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(events.events.len(), 1);
    assert_eq!(events.events[0].sequence, 2);

    let forbidden = JobStore::events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Internal),
            after_sequence: None,
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(forbidden.code.to_string(), "job_event.visibility_forbidden");

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
    let next = JobStore::create(&store, job_create()).await.unwrap();
    assert_eq!(next.job_id, JobId::new(Uuid::from_u128(1)));
}

#[tokio::test]
async fn fake_job_store_enforces_event_sequence() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();

    let skipped_first = JobStore::append_event(&store, progress_event(job.job_id, 2)).await;
    assert_eq!(
        skipped_first.unwrap_err().code.to_string(),
        "job_event.sequence_invalid"
    );

    JobStore::append_event(&store, progress_event(job.job_id, 1))
        .await
        .unwrap();

    let duplicate = JobStore::append_event(&store, progress_event(job.job_id, 1)).await;
    assert_eq!(
        duplicate.unwrap_err().code.to_string(),
        "job_event.sequence_invalid"
    );

    let skipped = JobStore::append_event(&store, progress_event(job.job_id, 3)).await;
    assert_eq!(
        skipped.unwrap_err().code.to_string(),
        "job_event.sequence_invalid"
    );

    JobStore::append_event(&store, progress_event(job.job_id, 2))
        .await
        .unwrap();
}

#[tokio::test]
async fn fake_job_store_defaults_events_to_public_visibility_and_clamps_limits() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    let mut internal = progress_event(job.job_id, 1);
    internal.visibility = Visibility::Internal;
    let mut public = progress_event(job.job_id, 2);
    public.visibility = Visibility::Public;
    JobStore::append_event(&store, internal).await.unwrap();
    JobStore::append_event(&store, public).await.unwrap();

    let events = JobStore::events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: None,
            after_sequence: None,
            since_sequence: None,
            limit: Some(u32::MAX),
            cursor: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(events.events.len(), 1);
    assert_eq!(events.events[0].visibility, Visibility::Public);
    assert_eq!(events.limit, crate::limits::MAX_PAGE_LIMIT);
}

#[tokio::test]
async fn fake_job_store_heartbeat_cannot_resurrect_terminal_job() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, job.job_id, LifecycleStatus::Completed).await;

    let err = JobStore::heartbeat(
        &store,
        JobHeartbeat {
            job_id: job.job_id,
            attempt: 1,
            worker_id: Some("late-worker".to_string()),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp("2026-07-01T00:10:00Z".to_string()),
            sequence: 0,
            last_progress_at: None,
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code.to_string(), "job.invalid_transition");
    assert_eq!(
        JobStore::get(&store, job.job_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        LifecycleStatus::Completed
    );
}

#[tokio::test]
async fn fake_job_store_recovery_honors_staleness_cutoff() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, job.job_id, LifecycleStatus::Running).await;
    JobStore::heartbeat(
        &store,
        JobHeartbeat {
            job_id: job.job_id,
            attempt: 1,
            worker_id: Some("fresh-worker".to_string()),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp("2026-07-01T00:00:10Z".to_string()),
            sequence: 0,
            last_progress_at: None,
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        },
    )
    .await
    .unwrap();

    let recovery = JobStore::recover(
        &store,
        JobRecoveryRequest {
            kind: Some(JobKind::Source),
            stale_before: None,
            limit: None,
            older_than_seconds: Some(360),
            dry_run: false,
            allow_without_cutoff: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(recovery.jobs_scanned, 0);
    assert_eq!(
        JobStore::get(&store, job.job_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        LifecycleStatus::Running
    );
}

#[tokio::test]
async fn fake_job_store_filters_events_by_visibility() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    let mut internal = progress_event(job.job_id, 1);
    internal.visibility = Visibility::Internal;
    let mut redacted = progress_event(job.job_id, 2);
    redacted.visibility = Visibility::Redacted;
    JobStore::append_event(&store, internal).await.unwrap();
    JobStore::append_event(&store, redacted).await.unwrap();

    let events = JobStore::events(
        &store,
        JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Redacted),
            after_sequence: None,
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(events.events.len(), 1);
    assert_eq!(events.events[0].visibility, Visibility::Redacted);
}

#[tokio::test]
async fn fake_job_store_controls_cancel_retry_recover_cleanup_and_artifacts() {
    let store = FakeJobWatchStore::new();
    let job = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, job.job_id, LifecycleStatus::Running).await;

    let cancel = JobStore::cancel(
        &store,
        job.job_id,
        JobCancelRequest {
            reason: Some("user requested".to_string()),
            force_after_ms: None,
            actor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(cancel.status, LifecycleStatus::Canceling);
    assert_eq!(cancel.reason.as_deref(), Some("user requested"));

    JobStore::update_status(&store, status_update(job.job_id, LifecycleStatus::Canceled))
        .await
        .unwrap();
    let retry = JobStore::retry(
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
    .unwrap();
    assert_eq!(retry.original_job_id, job.job_id);
    assert_eq!(retry.retry_job.status, LifecycleStatus::Queued);

    let running = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, running.job_id, LifecycleStatus::Running).await;
    let recovery = JobStore::recover(
        &store,
        JobRecoveryRequest {
            kind: Some(JobKind::Source),
            stale_before: None,
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(recovery.jobs_scanned, 1);
    assert_eq!(recovery.jobs_failed, 1);

    let artifacts = JobStore::artifacts(
        &store,
        JobArtifactListRequest {
            job_id: retry.retry_job.job_id,
            kind: None,
            limit: Some(5),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert!(artifacts.artifacts.is_empty());
    assert_eq!(artifacts.limit, 5);

    let cleanup = JobStore::cleanup(
        &store,
        JobCleanupRequest {
            kind: None,
            older_than: None,
            status: None,
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            confirm_all_terminal: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(cleanup.jobs_pruned, 1);
}

#[tokio::test]
async fn fake_job_store_delete_jobs_deletes_terminal_skips_live_reports_missing() {
    let store = FakeJobWatchStore::new();

    let terminal = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, terminal.job_id, LifecycleStatus::Completed).await;

    let live = JobStore::create(&store, job_create()).await.unwrap();
    drive_job_to(&store, live.job_id, LifecycleStatus::Running).await;

    let missing = JobId::new(Uuid::from_u128(424_242));

    let result = JobStore::delete_jobs(&store, &[terminal.job_id, live.job_id, missing])
        .await
        .unwrap();
    assert_eq!(result.deleted, vec![terminal.job_id]);
    assert_eq!(result.skipped_live, vec![live.job_id]);
    assert_eq!(result.missing, vec![missing]);

    assert!(
        JobStore::get(&store, terminal.job_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(JobStore::get(&store, live.job_id).await.unwrap().is_some());
}

#[tokio::test]
async fn fake_job_store_delete_jobs_is_noop_for_empty_input() {
    let store = FakeJobWatchStore::new();
    let result = JobStore::delete_jobs(&store, &[]).await.unwrap();
    assert_eq!(result, JobDeleteResult::default());
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

fn status_update(job_id: JobId, status: LifecycleStatus) -> JobStatusUpdate {
    JobStatusUpdate {
        job_id,
        source_id: None,
        status,
        phase: match status {
            LifecycleStatus::Queued => PipelinePhase::Queued,
            LifecycleStatus::Pending => PipelinePhase::Planning,
            LifecycleStatus::Waiting => PipelinePhase::Embedding,
            LifecycleStatus::Canceling | LifecycleStatus::Canceled => PipelinePhase::Canceled,
            LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded => {
                PipelinePhase::Complete
            }
            LifecycleStatus::Expired | LifecycleStatus::Skipped => PipelinePhase::Complete,
            _ => PipelinePhase::Embedding,
        },
        stage_id: None,
        counts: None,
        current: None,
        message: None,
        error: None,
    }
}

async fn drive_job_to(store: &FakeJobWatchStore, job_id: JobId, status: LifecycleStatus) {
    let steps: &[LifecycleStatus] = match status {
        LifecycleStatus::Queued => &[],
        LifecycleStatus::Pending => &[LifecycleStatus::Running, LifecycleStatus::Waiting],
        LifecycleStatus::Running => &[LifecycleStatus::Running],
        LifecycleStatus::Waiting => &[LifecycleStatus::Running, LifecycleStatus::Waiting],
        LifecycleStatus::Canceling => &[LifecycleStatus::Running, LifecycleStatus::Canceling],
        LifecycleStatus::Canceled => &[
            LifecycleStatus::Running,
            LifecycleStatus::Canceling,
            LifecycleStatus::Canceled,
        ],
        LifecycleStatus::Completed => &[LifecycleStatus::Running, LifecycleStatus::Completed],
        LifecycleStatus::CompletedDegraded => {
            &[LifecycleStatus::Running, LifecycleStatus::CompletedDegraded]
        }
        LifecycleStatus::Failed => &[LifecycleStatus::Running, LifecycleStatus::Failed],
        LifecycleStatus::Expired => &[LifecycleStatus::Queued, LifecycleStatus::Expired],
        LifecycleStatus::Skipped => &[LifecycleStatus::Running, LifecycleStatus::Completed],
        LifecycleStatus::Blocked => &[LifecycleStatus::Blocked],
    };

    for step in steps {
        JobStore::update_status(store, status_update(job_id, *step))
            .await
            .unwrap();
    }
}

fn progress_event(job_id: JobId, sequence: u64) -> SourceProgressEvent {
    SourceProgressEvent {
        event_id: format!("evt_{sequence}"),
        sequence,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility: Visibility::Internal,
        message: "progress".to_string(),
        timestamp: Timestamp(format!("2026-07-01T00:00:{sequence:02}Z")),
        source_id: None,
        canonical_uri: None,
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
    }
}

fn job_create() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_fake".to_string()),
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
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
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
            collection: None,
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

    let job = JobStore::create(&store, job_create()).await.unwrap();
    WatchStore::record_run(&store, watch.watch_id.clone(), job.job_id)
        .await
        .unwrap();
    let history = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: watch.watch_id,
            status: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(history.jobs.len(), 1);
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
            status: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "watch.not_found");
}
