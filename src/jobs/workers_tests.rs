use super::*;
use crate::jobs::backend::JobPayload;
use crate::jobs::cancel::CancelStore;
use crate::jobs::ops::enqueue_job;
use crate::jobs::store::{ReclaimedJobs, open_sqlite_pool};

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

    let handles = spawn_workers(pool, cfg, cancel_store);
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

#[test]
fn watchdog_reclaim_cancels_local_tokens_before_retry_notify() {
    let cancel_store = CancelStore::new();
    let id = uuid::Uuid::new_v4();
    let token = cancel_store.register(id, "attempt-1");
    let reclaimed = ReclaimedJobs {
        embed: vec![crate::jobs::store::ReclaimedJob {
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
        embed: vec![crate::jobs::store::ReclaimedJob {
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
