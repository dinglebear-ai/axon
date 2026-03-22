use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::jobs::common::claim_pending_by_id;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use sqlx::PgPool;
use std::collections::VecDeque;
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
    cfg: &Arc<Config>,
    pool: &PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: &Arc<tokio::sync::Semaphore>,
) -> Result<Option<Pin<Box<dyn Future<Output = ()>>>>, Box<dyn std::error::Error>> {
    let permit = semaphore.clone().acquire_owned().await?;
    match claim_pending_by_id(pool, wc.table, job_id).await {
        Ok(true) => {
            let fut = process_fn(Arc::clone(cfg), pool.clone(), job_id);
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
            Err(e.into())
        }
    }
}

/// Claim a delivery, ack/nack appropriately, and return the job future to push
/// into the in-flight set (if the job was successfully claimed) along with its
/// permit (which must be dropped when the job completes).
///
/// Uses `try_acquire_owned` (non-blocking) instead of `acquire_owned().await` to
/// prevent a TOCTOU race where another lane exhausts all semaphore permits between
/// the saturation check at the top of the lane loop and this call. Blocking here
/// would hold the delivery unacked for the full duration of a running job
/// (20–30 min for large ingest repos), which exceeds RabbitMQ's consumer_timeout
/// (1800 s) and kills the AMQP channel.
///
/// When no permit is immediately available the delivery is pre-acked (or nacked
/// with requeue if the buffer is full) following the same policy as the saturation
/// handler, and `job_id` is pushed to `preacked_ids` for processing once a permit
/// frees up.
///
/// Returns `Ok(Some(fut))` — job was claimed, caller pushes to inflight.
/// Returns `Ok(None)` — delivery was malformed, already claimed, or pre-acked;
///                       caller should continue the loop.
/// Returns `Err(_)` — ack/nack failed or semaphore closed; lane should exit.
#[expect(
    clippy::too_many_arguments,
    reason = "worker delivery dispatch requires all 9 distinct contexts; grouping into a struct would add indirection without improving clarity"
)]
pub(crate) async fn claim_delivery(
    delivery: lapin::message::Delivery,
    cfg: &Arc<Config>,
    pool: &PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: &Arc<tokio::sync::Semaphore>,
    preacked_ids: &mut VecDeque<Uuid>,
    preack_cap: usize,
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

    // Non-blocking permit check. If all permits are held by long-running jobs,
    // fall through to the pre-ack path rather than blocking with an unacked
    // delivery in hand.
    let permit = match semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            // No permit immediately available — pre-ack or nack to clear the
            // unacked slot without blocking (same policy as the saturation handler).
            if preacked_ids.len() < preack_cap {
                delivery.ack(BasicAckOptions::default()).await?;
                log_debug(&format!(
                    "{} worker lane={lane} late saturation pre-ack job_id={job_id}",
                    wc.job_kind
                ));
                preacked_ids.push_back(job_id);
            } else {
                log_warn(&format!(
                    "{} worker lane={lane} pre-ack buffer full (cap={preack_cap}), nacking late saturation delivery job_id={job_id}",
                    wc.job_kind
                ));
                delivery
                    .nack(BasicNackOptions {
                        requeue: true,
                        ..Default::default()
                    })
                    .await?;
            }
            return Ok(None);
        }
    };

    match claim_pending_by_id(pool, wc.table, job_id).await {
        Ok(true) => {
            delivery.ack(BasicAckOptions::default()).await?;
            let fut = process_fn(Arc::clone(cfg), pool.clone(), job_id);
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
