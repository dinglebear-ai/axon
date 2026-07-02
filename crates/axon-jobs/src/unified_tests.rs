use axon_api::source::*;

use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use crate::unified::SqliteUnifiedJobStore;

async fn store() -> SqliteUnifiedJobStore {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    seed_source(&pool).await;
    SqliteUnifiedJobStore::new(pool)
}

async fn seed_source(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO axon_source_sources (
            source_id, source_kind, collection, index_version,
            committed_generation, max_generation, updated_at_ms
        ) VALUES ('src_local', 'local_code', 'axon', 1, 0, 0, 1)",
    )
    .execute(pool)
    .await
    .expect("seed source row");
}

fn create_request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_local".to_string()),
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: Some(SourceId::new("src_local")),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        priority: JobPriority::Normal,
        idempotency_key: Some("idem-local".to_string()),
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Embedding,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(3),
        }],
        request: Some(serde_json::json!({"source": "/tmp/project"})),
        auth_snapshot: MetadataMap::new(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn migration_creates_canonical_job_tables() {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    let tables = [
        "jobs",
        "job_attempts",
        "job_stages",
        "job_events",
        "job_heartbeats",
        "provider_reservations",
        "job_artifacts",
    ];
    for table in tables {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("sqlite_master query");
        assert_eq!(count, 1, "{table} should exist");
    }
}

#[tokio::test]
async fn create_is_idempotent_and_get_returns_summary() {
    let store = store().await;
    let first = store.create(create_request()).await.expect("create job");
    let second = store
        .create(create_request())
        .await
        .expect("idempotent create");
    assert_eq!(first.job_id, second.job_id);

    let summary = store
        .get(first.job_id)
        .await
        .expect("get job")
        .expect("job exists");
    assert_eq!(summary.kind, JobKind::Source);
    assert_eq!(summary.intent, Some(JobIntent::Run));
    assert_eq!(summary.status, LifecycleStatus::Queued);
    assert_eq!(summary.phase, PipelinePhase::Queued);
    assert_eq!(summary.source_id, Some(SourceId::new("src_local")));

    store
        .create(JobCreateRequest {
            idempotency_key: Some("idem-local-second".to_string()),
            ..create_request()
        })
        .await
        .expect("create second job");
    let page = store
        .list(JobListRequest {
            status: Some(LifecycleStatus::Queued),
            kind: Some(JobKind::Source),
            source_id: None,
            watch_id: None,
            limit: Some(1),
            cursor: None,
        })
        .await
        .expect("list jobs");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, Some(2));
}

#[tokio::test]
async fn status_update_enforces_state_machine_and_persists_progress() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");

    let invalid = store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await;
    assert!(invalid.is_err(), "queued -> completed should be rejected");

    let counts = StageCounts {
        items_total: Some(2),
        items_done: 1,
        documents_total: Some(1),
        documents_done: 1,
        chunks_total: Some(4),
        chunks_done: 2,
        bytes_total: None,
        bytes_done: 0,
    };
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: Some(counts.clone()),
            current: Some(ProgressCurrent {
                source_item_key: Some(SourceItemKey::new("src/lib.rs")),
                document_id: None,
                chunk_id: None,
                adapter: Some("local".to_string()),
                provider: Some(ProviderId::new("tei")),
                message: Some("embedding src/lib.rs".to_string()),
            }),
            message: Some("running".to_string()),
            error: None,
        })
        .await
        .expect("queued -> running");

    let summary = store
        .get(job.job_id)
        .await
        .expect("get job")
        .expect("job exists");
    assert_eq!(summary.status, LifecycleStatus::Running);
    assert_eq!(summary.phase, PipelinePhase::Embedding);
    assert_eq!(summary.counts, Some(counts.clone()));
    assert!(summary.started_at.is_some());

    let stage = store
        .stages(job.job_id)
        .await
        .expect("stages")
        .into_iter()
        .next()
        .expect("stage plan created");
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Waiting,
            phase: PipelinePhase::Embedding,
            stage_id: Some(stage.stage_id),
            counts: Some(counts.clone()),
            current: None,
            message: Some("waiting on provider".to_string()),
            error: None,
        })
        .await
        .expect("running -> waiting updates stage");

    let stage = store
        .stages(job.job_id)
        .await
        .expect("stages")
        .into_iter()
        .next()
        .expect("stage exists");
    assert_eq!(stage.status, LifecycleStatus::Waiting);
    assert_eq!(stage.counts, counts);
}

#[tokio::test]
async fn append_event_requires_monotonic_sequences_and_filters_events() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");

    let skipped = store
        .append_event(progress_event(job.job_id, 2, Visibility::Public))
        .await;
    assert!(skipped.is_err(), "first event must be sequence 1");

    store
        .append_event(progress_event(job.job_id, 1, Visibility::Internal))
        .await
        .expect("append first event");
    store
        .append_event(progress_event(job.job_id, 2, Visibility::Public))
        .await
        .expect("append second event");

    let public_events = store
        .events(JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list events");
    assert_eq!(public_events.events.len(), 1);
    assert_eq!(public_events.events[0].sequence, 2);
    assert_eq!(public_events.last_sequence, Some(2));

    let default_events = store
        .events(JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: None,
            since_sequence: None,
            limit: Some(u32::MAX),
            cursor: None,
        })
        .await
        .expect("list default-visible events");
    assert_eq!(default_events.events.len(), 1);
    assert_eq!(default_events.events[0].visibility, Visibility::Public);
    assert_eq!(default_events.limit, crate::limits::MAX_PAGE_LIMIT);

    let mut duplicate = progress_event(job.job_id, 3, Visibility::Public);
    duplicate.event_id = "event-dedupe-a".to_string();
    duplicate.dedupe_key = Some("embedding:src/lib.rs".to_string());
    store
        .append_event(duplicate.clone())
        .await
        .expect("append dedupe event");
    duplicate.event_id = "event-dedupe-b".to_string();
    store
        .append_event(duplicate)
        .await
        .expect("duplicate dedupe event is idempotent");
    let mut next_duplicate = progress_event(job.job_id, 4, Visibility::Public);
    next_duplicate.event_id = "event-dedupe-c".to_string();
    next_duplicate.dedupe_key = Some("embedding:src/lib.rs".to_string());
    store
        .append_event(next_duplicate)
        .await
        .expect("expected duplicate dedupe event still consumes sequence");

    let public_events = store
        .events(JobEventListRequest {
            job_id: job.job_id,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list events");
    assert_eq!(public_events.events.len(), 3);
    assert_eq!(public_events.events[1].sequence, 3);
    assert_eq!(public_events.events[2].sequence, 4);
    assert_eq!(
        public_events.events[2].details.get("dedupe_duplicate"),
        Some(&serde_json::json!(true))
    );
    let mut gap_duplicate = progress_event(job.job_id, 99, Visibility::Public);
    gap_duplicate.event_id = "event-dedupe-gap".to_string();
    gap_duplicate.dedupe_key = Some("embedding:src/lib.rs".to_string());
    let gap = store.append_event(gap_duplicate).await.unwrap_err();
    assert_eq!(gap.code.to_string(), "job_event.sequence_invalid");
}

#[tokio::test]
async fn heartbeat_updates_latest_job_summary_and_history() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    let heartbeat = JobHeartbeat {
        job_id: job.job_id,
        attempt: 1,
        worker_id: Some("worker-a".to_string()),
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        stage_id: None,
        heartbeat_at: Timestamp("2026-07-01T12:00:00Z".to_string()),
        last_event_sequence: Some(7),
        counts: None,
        provider_reservations: vec![ProviderReservationSnapshot {
            reservation_id: ReservationId::new("res_test"),
            provider_kind: ProviderKind::Embedding,
            provider_id: Some(ProviderId::new("tei")),
            priority: JobPriority::Background,
            requested_units: 2,
            granted_units: 1,
            acquired_at: Some(Timestamp("2026-07-01T11:59:59Z".to_string())),
            expires_at: Some(Timestamp("2026-07-01T12:05:00Z".to_string())),
            status: ProviderReservationStatus::Active,
            queue_depth: Some(3),
            cooling: None,
        }],
    };

    store.heartbeat(heartbeat.clone()).await.expect("heartbeat");

    let summary = store
        .get(job.job_id)
        .await
        .expect("get job")
        .expect("job exists");
    assert_eq!(summary.phase, PipelinePhase::Embedding);
    assert_eq!(summary.status, LifecycleStatus::Running);
    assert_eq!(summary.attempt, 1);
    assert_eq!(summary.heartbeat, Some(heartbeat));
    let reservation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM provider_reservations WHERE job_id = ?")
            .bind(job.job_id.0.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("reservation count");
    assert_eq!(reservation_count, 1);
}

#[tokio::test]
async fn heartbeat_cannot_resurrect_terminal_job() {
    let store = store().await;
    let job = store.create(create_request()).await.expect("create job");
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("running");
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("completed");

    let err = store
        .heartbeat(JobHeartbeat {
            job_id: job.job_id,
            attempt: 1,
            worker_id: Some("late-worker".to_string()),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp("2026-07-01T12:05:00Z".to_string()),
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(err.code.to_string(), "job.invalid_transition");
    assert_eq!(
        store.get(job.job_id).await.unwrap().unwrap().status,
        LifecycleStatus::Completed
    );
}

#[tokio::test]
async fn recovery_honors_staleness_cutoff() {
    let store = store().await;
    let job = store
        .create(JobCreateRequest {
            idempotency_key: Some("fresh-recovery-cutoff".to_string()),
            ..create_request()
        })
        .await
        .expect("create job");
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("running");
    store
        .heartbeat(JobHeartbeat {
            job_id: job.job_id,
            attempt: 1,
            worker_id: Some("fresh-worker".to_string()),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp::from(chrono::Utc::now()),
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        })
        .await
        .expect("fresh heartbeat");

    let recovery = store
        .recover(JobRecoveryRequest {
            kind: Some(JobKind::Source),
            older_than_seconds: Some(360),
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .expect("recover");

    assert_eq!(recovery.jobs_scanned, 0);
    assert_eq!(
        store.get(job.job_id).await.unwrap().unwrap().status,
        LifecycleStatus::Running
    );
}

#[tokio::test]
async fn control_operations_cancel_retry_recover_cleanup_and_list_artifacts() {
    let store = store().await;
    let queued = store
        .create(JobCreateRequest {
            idempotency_key: Some("cancel-queued".to_string()),
            ..create_request()
        })
        .await
        .expect("create queued job");
    let queued_cancel = store
        .cancel(
            queued.job_id,
            JobCancelRequest {
                reason: Some("queued no longer needed".to_string()),
                force_after_ms: None,
            },
        )
        .await
        .expect("cancel queued");
    assert_eq!(queued_cancel.status, LifecycleStatus::Canceled);
    assert!(queued_cancel.canceled_at.is_some());

    let job = store.create(create_request()).await.expect("create job");
    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("running");

    let cancel = store
        .cancel(
            job.job_id,
            JobCancelRequest {
                reason: Some("user requested".to_string()),
                force_after_ms: None,
            },
        )
        .await
        .expect("cancel");
    assert_eq!(cancel.status, LifecycleStatus::Canceling);
    assert_eq!(cancel.reason.as_deref(), Some("user requested"));

    store
        .update_status(JobStatusUpdate {
            job_id: job.job_id,
            status: LifecycleStatus::Canceled,
            phase: PipelinePhase::Canceled,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("canceled");
    let retry = store
        .retry(
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
    assert_eq!(retry.retry_job.status, LifecycleStatus::Queued);
    let retry_stages = store
        .stages(retry.retry_job.job_id)
        .await
        .expect("retry stages");
    assert_eq!(retry_stages.len(), 1);
    let retry_request: Option<String> =
        sqlx::query_scalar("SELECT request_json FROM jobs WHERE job_id = ?")
            .bind(retry.retry_job.job_id.0.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("retry request");
    assert_eq!(
        retry_request.as_deref(),
        Some("{\"source\":\"/tmp/project\"}")
    );

    sqlx::query(
        "INSERT INTO job_artifacts (
            artifact_id, job_id, artifact_kind, uri, size_bytes, content_hash, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("artifact-local-path")
    .bind(job.job_id.0.to_string())
    .bind("report")
    .bind("file:///home/jmagar/.axon/artifacts/private/report.json")
    .bind(128_i64)
    .bind("sha256:abc")
    .bind("2026-07-01T12:30:00Z")
    .execute(&store.pool)
    .await
    .expect("insert artifact");
    let artifacts = store
        .artifacts(JobArtifactListRequest {
            job_id: job.job_id,
            kind: None,
            limit: Some(7),
            cursor: None,
        })
        .await
        .expect("artifacts");
    assert_eq!(artifacts.artifacts.len(), 1);
    assert_eq!(artifacts.artifacts[0].uri, "artifact://artifact-local-path");
    assert_eq!(artifacts.limit, 7);

    let running = store
        .create(JobCreateRequest {
            idempotency_key: Some("recover-running".to_string()),
            ..create_request()
        })
        .await
        .expect("create running job");
    store
        .update_status(JobStatusUpdate {
            job_id: running.job_id,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("running");
    store
        .heartbeat(JobHeartbeat {
            job_id: running.job_id,
            attempt: 1,
            worker_id: Some("recover-worker".to_string()),
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp("2026-07-01T12:00:00Z".to_string()),
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        })
        .await
        .expect("running heartbeat");
    let recovery = store
        .recover(JobRecoveryRequest {
            kind: Some(JobKind::Source),
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: true,
        })
        .await
        .expect("recover");
    assert_eq!(recovery.jobs_scanned, 1);
    assert_eq!(recovery.jobs_failed, 1);
    let attempts = store.attempts(running.job_id).await.expect("attempts");
    assert_eq!(attempts[0].status, LifecycleStatus::Failed);
    assert!(attempts[0].finished_at.is_some());
    let recovered = store
        .get(running.job_id)
        .await
        .expect("get recovered")
        .expect("recovered job");
    assert_eq!(recovered.status, LifecycleStatus::Failed);
    assert_eq!(
        recovered
            .heartbeat
            .as_ref()
            .map(|heartbeat| heartbeat.status),
        Some(LifecycleStatus::Failed)
    );
    assert!(
        store
            .stages(running.job_id)
            .await
            .expect("recovered stages")
            .iter()
            .all(|stage| stage.status == LifecycleStatus::Failed)
    );
    sqlx::query(
        "INSERT INTO job_artifacts (
            artifact_id, job_id, artifact_kind, uri, size_bytes, content_hash, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("artifact-recovered-job")
    .bind(running.job_id.0.to_string())
    .bind("report")
    .bind("file:///home/jmagar/.axon/artifacts/private/recovered.json")
    .bind(64_i64)
    .bind("sha256:def")
    .bind("2026-07-01T12:31:00Z")
    .execute(&store.pool)
    .await
    .expect("insert recovered artifact");

    let cleanup = store
        .cleanup(JobCleanupRequest {
            older_than_seconds: None,
            dry_run: false,
            confirm_all_terminal: true,
        })
        .await
        .expect("cleanup");
    assert_eq!(cleanup.jobs_pruned, 2);
    assert_eq!(cleanup.artifacts_pruned, 1);
    for table in [
        "job_events",
        "job_heartbeats",
        "job_attempts",
        "job_stages",
        "job_artifacts",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE job_id = ?");
        let remaining = sqlx::query_scalar::<_, i64>(&sql)
            .bind(running.job_id.0.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("count child rows");
        assert_eq!(remaining, 0, "{table} rows should be pruned");
    }
}

fn progress_event(job_id: JobId, sequence: u64, visibility: Visibility) -> SourceProgressEvent {
    SourceProgressEvent {
        event_id: format!("event-{sequence}"),
        sequence,
        job_id,
        attempt: 0,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility,
        message: format!("event {sequence}"),
        timestamp: Timestamp(format!("2026-07-01T00:00:0{sequence}Z")),
        source_id: None,
        canonical_uri: None,
        adapter: None,
        scope: None,
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
        warning: None,
        error: None,
    }
}
