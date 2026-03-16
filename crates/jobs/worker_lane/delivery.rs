use crate::crates::core::logging::log_warn;
use crate::crates::jobs::common::claim_pending_by_id;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

use super::{ProcessFn, WorkerConfig};
use crate::crates::core::config::Config;

/// Claim a job whose AMQP delivery was already acked during saturation to prevent
/// consumer_timeout. Acquires a semaphore permit, claims the DB row, and returns
/// the job future.
///
/// Returns `Ok(None)` if the job was already claimed by another lane or the DB
/// claim failed (job stays `pending` and is reclaimed by watchdog/startup sweep).
pub(crate) async fn claim_preacked_job(
    job_id: Uuid,
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: &Arc<tokio::sync::Semaphore>,
) -> Result<Option<Pin<Box<dyn Future<Output = ()>>>>, Box<dyn std::error::Error>> {
    let permit = semaphore.clone().acquire_owned().await?;
    match claim_pending_by_id(pool, wc.table, job_id).await {
        Ok(true) => {
            let fut = process_fn(cfg.clone(), pool.clone(), job_id);
            Ok(Some(Box::pin(async move {
                fut.await;
                drop(permit);
            })))
        }
        Ok(false) => {
            drop(permit);
            Ok(None)
        }
        Err(e) => {
            drop(permit);
            log_warn(&format!(
                "{} worker lane={lane} DB error claiming pre-acked job {job_id}: {e}",
                wc.job_kind
            ));
            Ok(None)
        }
    }
}

/// Claim a delivery, ack/nack appropriately, and return the job future to push
/// into the in-flight set (if the job was successfully claimed) along with its
/// permit (which must be dropped when the job completes).
///
/// Returns `Ok(Some(fut))` — job was claimed, caller pushes to inflight.
/// Returns `Ok(None)` — delivery was malformed or already claimed; acked+skipped.
/// Returns `Err(_)` — ack/nack failed or semaphore closed; lane should exit.
///
/// The returned future is NOT `Send` because `ProcessFn` returns a `!Send`
/// future.  This is fine — the entire lane runs on a single async task.
pub(crate) async fn claim_delivery(
    delivery: lapin::message::Delivery,
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: &Arc<tokio::sync::Semaphore>,
) -> Result<Option<Pin<Box<dyn Future<Output = ()>>>>, Box<dyn std::error::Error>> {
    let parsed = std::str::from_utf8(&delivery.data)
        .ok()
        .and_then(|s| Uuid::parse_str(s.trim()).ok());
    let Some(job_id) = parsed else {
        log_warn(&format!(
            "{} worker lane={lane} malformed delivery payload (len={}), acking and skipping",
            wc.job_kind,
            delivery.data.len()
        ));
        delivery.ack(BasicAckOptions::default()).await?;
        return Ok(None);
    };

    // Reserve capacity first so we never claim a job without a runnable slot.
    let permit = semaphore.clone().acquire_owned().await?;
    match claim_pending_by_id(pool, wc.table, job_id).await {
        Ok(true) => {
            delivery.ack(BasicAckOptions::default()).await?;
            let fut = process_fn(cfg.clone(), pool.clone(), job_id);
            Ok(Some(Box::pin(async move {
                fut.await;
                drop(permit);
            })))
        }
        Ok(false) => {
            drop(permit);
            // Another lane claimed this ID first; ack and skip.
            delivery.ack(BasicAckOptions::default()).await?;
            Ok(None)
        }
        Err(e) => {
            drop(permit);
            log_warn(&format!(
                "{} worker lane={lane} DB error claiming job {job_id}; nacking for retry: {e}",
                wc.job_kind
            ));
            if let Err(nack_err) = delivery
                .nack(BasicNackOptions {
                    requeue: true,
                    ..Default::default()
                })
                .await
            {
                log_warn(&format!(
                    "{} worker lane={lane} failed to nack delivery: {nack_err}",
                    wc.job_kind
                ));
            }
            Ok(None)
        }
    }
}
