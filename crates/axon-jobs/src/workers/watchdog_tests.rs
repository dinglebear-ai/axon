use super::*;

use crate::boundary::JobStore;
use crate::cancel::CancelStore;
use crate::store::open_sqlite_pool;
use crate::store::{ReclaimedJob, ReclaimedJobs};
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    JobStagePlan, LifecycleStatus, MetadataMap, PipelinePhase, Timestamp,
};
use tempfile::NamedTempFile;
use tokio::sync::Notify;

#[test]
fn watchdog_reclaim_cancels_local_tokens_before_retry_notify() {
    let cancel_store = CancelStore::new();
    let id = uuid::Uuid::new_v4();
    let token = cancel_store.register(id, "attempt-1");
    let reclaimed = ReclaimedJobs {
        embed: vec![ReclaimedJob {
            id,
            attempt_id: Some("attempt-1".to_string()),
        }],
        ..Default::default()
    };

    cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);

    assert!(token.is_cancelled(), "old local owner must be canceled");
    assert!(
        !cancel_store.cancel_local(id, "attempt-1"),
        "token should be removed after watchdog local cancel"
    );
}

#[test]
fn watchdog_reclaim_does_not_cancel_new_attempt_token() {
    let cancel_store = CancelStore::new();
    let id = uuid::Uuid::new_v4();
    let old_token = cancel_store.register(id, "attempt-1");
    let new_token = cancel_store.register(id, "attempt-2");
    let reclaimed = ReclaimedJobs {
        embed: vec![ReclaimedJob {
            id,
            attempt_id: Some("attempt-1".to_string()),
        }],
        ..Default::default()
    };

    cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);

    assert!(old_token.is_cancelled(), "stale attempt should be canceled");
    assert!(
        !new_token.is_cancelled(),
        "fresh retry attempt must not be canceled by stale reclaim"
    );
}

fn test_notifies() -> WatchdogNotifies {
    WatchdogNotifies {
        crawl: Arc::new(Notify::new()),
        embed: Arc::new(Notify::new()),
        extract: Arc::new(Notify::new()),
        ingest: Arc::new(Notify::new()),
        unified: Arc::new(Notify::new()),
    }
}

/// Regression test: a unified job stuck in `running` past the stale
/// threshold (simulating a process crash, OOM-kill, or a panic-guard bypass)
/// must be reclaimed automatically by the watchdog sweep — not just via the
/// on-demand `crawl recover`/`embed recover`/etc. CLI/MCP paths.
#[tokio::test]
async fn watchdog_sweep_reclaims_stale_unified_job() {
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let pool = Arc::new(pool);

    let store = SqliteUnifiedJobStore::new((*pool).clone());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: UnifiedJobKind::Memory,
            job_intent: JobIntent::Run,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![JobStagePlan {
                phase: PipelinePhase::Fetching,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: None,
            }],
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: None,
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .unwrap();
    let job_id = descriptor.job_id;

    // Force the job into `running` with an `updated_at` far in the past —
    // simulating a worker that claimed it, then crashed/panicked without
    // ever writing a terminal state or heartbeat.
    let stale_updated_at = Timestamp::from(chrono::Utc::now() - chrono::Duration::hours(1));
    sqlx::query(
        "UPDATE jobs SET status = 'running', started_at = ?, updated_at = ? WHERE job_id = ?",
    )
    .bind(stale_updated_at.0.as_str())
    .bind(stale_updated_at.0.as_str())
    .bind(job_id.0.to_string())
    .execute(&*pool)
    .await
    .unwrap();

    let mut cfg = Config::default_minimal();
    // Tight thresholds so a 1-hour-stale job is reclaimed on the first tick,
    // and a short sweep interval so the test doesn't have to wait long.
    cfg.watchdog_stale_timeout_secs = 1;
    cfg.watchdog_confirm_secs = 0;
    cfg.watchdog_sweep_secs = 1;
    cfg.worker_starvation_secs = 3600;
    let cfg = Arc::new(cfg);

    let cancel_store = Arc::new(CancelStore::new());
    let notifies = test_notifies();
    let unified_notify = Arc::clone(&notifies.unified);
    let shutdown = CancellationToken::new();

    let handle = tokio::spawn(watchdog_loop(
        Arc::clone(&pool),
        cfg,
        cancel_store,
        notifies,
        shutdown.clone(),
    ));

    // Wait for the watchdog to wake this job's Notify handle (bounded so a
    // regression hangs the test instead of looping forever).
    tokio::time::timeout(Duration::from_secs(5), unified_notify.notified())
        .await
        .expect("watchdog should reclaim the stale unified job and notify within 5s");

    shutdown.cancel();
    let _ = handle.await;

    let summary = store.get(job_id).await.unwrap().unwrap();
    assert_eq!(
        summary.status,
        LifecycleStatus::Queued,
        "reclaimed unified job must be requeued, not left stuck running"
    );
}
