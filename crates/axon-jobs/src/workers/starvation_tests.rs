use super::*;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::backend::JobPayload;
use crate::ops::{claim_next_pending_for_attempt, enqueue_job};
use crate::store::open_sqlite_pool;
use axon_core::config::Config;

const EMBED_TABLE: &str = "axon_embed_jobs";

fn test_notifies() -> WatchdogNotifies {
    WatchdogNotifies {
        crawl: Arc::new(Notify::new()),
        embed: Arc::new(Notify::new()),
        extract: Arc::new(Notify::new()),
        ingest: Arc::new(Notify::new()),
        unified: Arc::new(Notify::new()),
    }
}

async fn enqueue_pending_embed(pool: &SqlitePool) -> uuid::Uuid {
    enqueue_job(
        pool,
        &JobPayload::Embed {
            input: "starvation-test".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .unwrap()
}

async fn set_created_at(pool: &SqlitePool, id: uuid::Uuid, created_at: i64) {
    sqlx::query(&format!(
        "UPDATE {EMBED_TABLE} SET created_at = ? WHERE id = ?"
    ))
    .bind(created_at)
    .bind(id.to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// Transition a freshly-enqueued embed job to `running` via the lifecycle claim
/// path (rather than a raw status UPDATE), honoring the job-state contract.
async fn claim_one_running(pool: &SqlitePool) -> uuid::Uuid {
    let id = enqueue_pending_embed(pool).await;
    let claimed = claim_next_pending_for_attempt(pool, JobKind::Embed)
        .await
        .unwrap()
        .expect("a pending embed job to claim");
    assert_eq!(claimed.id, id, "claim should pick the only pending job");
    id
}

#[tokio::test]
async fn alarms_when_pending_starves_with_no_running_worker() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let id = enqueue_pending_embed(&pool).await;
    set_created_at(&pool, id, now_ms() - 600_000).await; // 10 minutes old

    let alarms = detect_and_recover_starvation(&pool, &test_notifies(), 120_000).await;

    assert_eq!(alarms.len(), 1, "exactly the embed queue should alarm");
    assert_eq!(alarms[0].kind, JobKind::Embed);
    assert_eq!(alarms[0].pending, 1);
    assert!(
        alarms[0].oldest_age_ms >= 120_000,
        "age {} below threshold",
        alarms[0].oldest_age_ms
    );
}

#[tokio::test]
async fn fires_notify_for_the_starving_kind() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let id = enqueue_pending_embed(&pool).await;
    set_created_at(&pool, id, now_ms() - 600_000).await;

    let notifies = test_notifies();
    // Register a waiter deterministically BEFORE detection: poll the notified()
    // future once so the waiter is parked when notify_waiters() fires.
    let mut parked = Box::pin(notifies.embed.notified());
    let _ = futures::poll!(parked.as_mut());

    let alarms = detect_and_recover_starvation(&pool, &notifies, 120_000).await;
    assert_eq!(alarms.len(), 1);

    tokio::time::timeout(Duration::from_secs(2), parked.as_mut())
        .await
        .expect("starving embed lane should have been notified");
}

#[tokio::test]
async fn no_alarm_when_a_job_of_the_kind_is_running() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();

    // A lane IS working: claim a job into `running` while it is the only pending
    // row, then add an old pending job queued behind it.
    claim_one_running(&pool).await;
    let pending = enqueue_pending_embed(&pool).await;
    set_created_at(&pool, pending, now_ms() - 600_000).await; // old pending behind a busy lane

    let alarms = detect_and_recover_starvation(&pool, &test_notifies(), 120_000).await;
    assert!(
        alarms.iter().all(|a| a.kind != JobKind::Embed),
        "a backlog behind a busy lane is not starvation"
    );
}

#[tokio::test]
async fn no_alarm_when_pending_is_younger_than_threshold() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    enqueue_pending_embed(&pool).await; // created_at = now (fresh)

    let alarms = detect_and_recover_starvation(&pool, &test_notifies(), 120_000).await;
    assert!(
        alarms.is_empty(),
        "the normal enqueue->claim window must never false-alarm"
    );
}

#[tokio::test]
async fn disabled_when_threshold_is_zero() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let id = enqueue_pending_embed(&pool).await;
    set_created_at(&pool, id, now_ms() - 600_000).await;

    let alarms = detect_and_recover_starvation(&pool, &test_notifies(), 0).await;
    assert!(alarms.is_empty(), "threshold 0 disables the detector");
}

#[test]
fn starvation_alarm_projects_to_structured_warning_event() {
    let job_id = JobId::new(uuid::Uuid::new_v4());
    let event = StarvationAlarm {
        kind: JobKind::Embed,
        pending: 3,
        oldest_age_ms: 180_000,
    }
    .to_progress_event(job_id, 9, Timestamp("2026-07-01T00:00:00Z".to_string()));

    assert_eq!(event.job_id, job_id);
    assert_eq!(event.sequence, 9);
    assert_eq!(event.phase, PipelinePhase::Leasing);
    assert_eq!(event.status, LifecycleStatus::Waiting);
    assert_eq!(event.severity, Severity::Warning);
    assert_eq!(event.counts.items_total, Some(3));
    assert_eq!(
        event.warning.as_ref().map(|warning| warning.code.as_str()),
        Some("worker.starvation")
    );
    assert_eq!(
        event.dedupe_key.as_deref(),
        Some("starvation:axon_embed_jobs")
    );
}
