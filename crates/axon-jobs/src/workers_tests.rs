use super::*;
use crate::backend::JobPayload;
use crate::boundary::JobStore;
use crate::cancel::CancelStore;
use crate::ops::enqueue_job;
use crate::store::open_sqlite_pool;
use crate::unified::SqliteUnifiedJobStore;
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind,
    JobPriority, JobRecoveryRequest, JobStagePlan, LifecycleStatus, MetadataMap, PipelinePhase,
    SourceId,
};
use sqlx::SqlitePool;

#[tokio::test]
async fn worker_picks_up_job_via_notify() {
    let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
    let notify = Arc::new(Notify::new());

    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "test content".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .unwrap();

    let pool2 = Arc::clone(&pool);
    let notify2 = Arc::clone(&notify);
    let (tx, rx) = tokio::sync::oneshot::channel::<uuid::Uuid>();
    tokio::spawn(async move {
        if let Some(claimed) = claim_next_pending_for_attempt(&pool2, JobKind::Embed)
            .await
            .unwrap()
        {
            assert_eq!(claimed.id, id);
            notify2.notify_one();
            let _ = tx.send(claimed.id);
        }
    });

    notify.notify_one();
    let claimed = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("task did not complete within 5s")
        .expect("sender dropped without sending");
    assert_eq!(claimed, id);

    let row: (String,) = sqlx::query_as("SELECT status FROM axon_embed_jobs WHERE id=?")
        .bind(id.to_string())
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_ne!(row.0, "pending", "job should have been claimed");
}

#[tokio::test]
async fn dropping_worker_handles_gracefully_stops_worker_loops() {
    let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
    let cfg = Arc::new(Config::default_minimal());
    let cancel_store = Arc::new(CancelStore::new());

    let handles = spawn_workers(pool, cfg, cancel_store, None);
    let abort_handles: Vec<_> = handles
        .worker_handles
        .iter()
        .map(tokio::task::JoinHandle::abort_handle)
        .collect();

    drop(handles);

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if abort_handles
                .iter()
                .all(tokio::task::AbortHandle::is_finished)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("worker tasks should stop when WorkerHandles is dropped");
}

#[tokio::test]
async fn unified_worker_claims_queued_job_from_durable_rows() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let job = store
        .create(unified_job_request(UnifiedJobKind::Source))
        .await
        .unwrap();

    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("queued unified job should be claimed");

    assert_eq!(claimed.job_id, job.job_id);
    assert_eq!(claimed.kind, UnifiedJobKind::Source);
    assert_eq!(
        claimed.request_json,
        Some(serde_json::json!({"operation": "source"}))
    );
    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Running);
    assert_eq!(summary.phase, PipelinePhase::Planning);
}

#[tokio::test]
async fn unified_worker_marks_unsupported_stage_failed() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let job = store
        .create(unified_job_request(UnifiedJobKind::Research))
        .await
        .unwrap();
    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("claim job");

    unified::run_unified_claimed(
        &pool,
        &Config::default_minimal(),
        &claimed,
        &CancellationToken::new(),
        None,
    )
    .await;

    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Failed);
    assert!(summary.last_error.is_some());
    let events = store
        .events(axon_api::source::JobEventListRequest {
            job_id: job.job_id,
            after_sequence: None,
            limit: Some(10),
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await
        .unwrap();
    assert!(
        events
            .events
            .iter()
            .any(|event| event.message.contains("not wired yet"))
    );
}

#[tokio::test]
async fn unified_worker_executes_extract_job_from_request_json() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default_minimal();
    cfg.output_dir = dir.path().to_path_buf();
    let config_json = crate::config_snapshot::extract_config_json(&cfg, None).unwrap();

    let mut request = unified_job_request(UnifiedJobKind::Extract);
    // Empty URLs so `extract_sync` completes deterministically with zero
    // items and no network access — this test proves real dispatch (not the
    // `unsupported_stage` catch-all), not extraction quality.
    request.request = Some(serde_json::json!({
        "urls": Vec::<String>::new(),
        "config_json": config_json,
    }));
    let job = store.create(request).await.unwrap();

    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("claim job");
    unified::run_unified_claimed(&pool, &cfg, &claimed, &CancellationToken::new(), None).await;

    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Completed);
    assert!(summary.last_error.is_none());
}

#[tokio::test]
async fn unified_worker_shutdown_claim_marks_job_canceled() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let job = store
        .create(unified_job_request(UnifiedJobKind::Source))
        .await
        .unwrap();
    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("claim job");
    let shutdown = CancellationToken::new();
    shutdown.cancel();

    unified::run_unified_claimed(&pool, &Config::default_minimal(), &claimed, &shutdown, None)
        .await;

    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Canceled);
    assert_eq!(summary.phase, PipelinePhase::Canceled);
}

#[tokio::test]
async fn stale_recovery_does_not_double_publish_when_original_attempt_is_alive() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let job = store
        .create(unified_job_request(UnifiedJobKind::Source))
        .await
        .unwrap();
    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("claim job");
    assert_eq!(claimed.job_id, job.job_id);

    let recovered = store
        .recover(JobRecoveryRequest {
            kind: Some(UnifiedJobKind::Source),
            stale_before: Some(axon_api::source::Timestamp::from(
                chrono::Utc::now() - chrono::Duration::seconds(60),
            )),
            limit: Some(10),
            older_than_seconds: Some(60),
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .unwrap();

    assert_eq!(recovered.recovered, 0);
    assert!(recovered.job_ids.is_empty());
    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Running);
}

#[tokio::test]
async fn stale_claimed_attempt_cannot_terminalize_recovered_job() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let job = store
        .create(unified_job_request(UnifiedJobKind::Source))
        .await
        .unwrap();
    let claimed = unified::claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("claim job");

    let recovered = store
        .recover(JobRecoveryRequest {
            kind: Some(UnifiedJobKind::Source),
            stale_before: Some(axon_api::source::Timestamp::from(
                chrono::Utc::now() + chrono::Duration::seconds(60),
            )),
            limit: Some(10),
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .unwrap();
    assert_eq!(recovered.recovered, 1);

    unified::run_unified_claimed(
        &pool,
        &Config::default_minimal(),
        &claimed,
        &CancellationToken::new(),
        None,
    )
    .await;

    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Queued);
    assert_eq!(summary.attempt, 2);
    assert!(
        store
            .stages(job.job_id)
            .await
            .unwrap()
            .iter()
            .all(|stage| stage.status == LifecycleStatus::Queued)
    );
}

async fn seed_source(pool: &SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO sources (
            source_id, committed_generation, summary_json, created_at, updated_at
        ) VALUES ('src_worker', NULL, '{}', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .expect("seed source row");
}

fn unified_job_request(kind: UnifiedJobKind) -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req-worker".to_string()),
        job_kind: kind,
        job_intent: JobIntent::Run,
        source_id: Some(SourceId::new("src_worker")),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Planning,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        request: Some(serde_json::json!({"operation": format!("{kind:?}").to_lowercase()})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_worker")),
        requirements: MetadataMap::new(),
        result_schema: Some("job_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    }
}

// `cancel_reclaimed_local_tokens` and its tests moved to `workers/watchdog.rs`
// + `workers/watchdog_tests.rs` when the watchdog loop was extracted.
