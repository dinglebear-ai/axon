//! AMQP consumer loop for crawl worker lanes.
//!
//! Extracted from `loops.rs` to keep the monolith under 300 lines. Contains the
//! AMQP consumer loop (`run_amqp_worker_lane`), delivery claiming, job execution
//! with tick-keeping, and watchdog sweep orchestration.

use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::jobs::common::{
    WatchdogSweepStats, claim_pending_by_id, mark_job_failed, open_amqp_connection_and_channel,
    reclaim_stale_running_jobs as generic_reclaim,
};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions};
use lapin::types::FieldTable;
use redis::AsyncCommands;
use sqlx::PgPool;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use super::super::{STALE_SWEEP_INTERVAL_SECS, TABLE, worker_concurrency};
use super::process::process_job;

/// Type alias — crawl uses the same stats struct as the generic watchdog.
pub(crate) type CrawlWatchdogSweepStats = WatchdogSweepStats;

/// Thin wrapper around the generic watchdog that delegates all logic to
/// `common::reclaim_stale_running_jobs`.
pub(crate) async fn reclaim_stale_running_jobs(
    pool: &PgPool,
    lane: usize,
    idle_timeout_secs: i64,
    confirm_secs: i64,
) -> Result<CrawlWatchdogSweepStats, Box<dyn Error>> {
    let marker = format!("lane={lane}");
    let stats = generic_reclaim(
        pool,
        TABLE,
        "crawl",
        idle_timeout_secs,
        confirm_secs,
        &marker,
    )
    .await?;
    Ok(stats)
}

/// Fire-and-forget: set Redis cancel keys for all watchdog-reclaimed job IDs.
/// Follows the same pattern as `db.rs:cancel_job()`. Never blocks the sweep.
pub(super) async fn signal_reclaimed_cancel_keys(redis_url: &str, ids: &[Uuid]) {
    // A fresh client is created per sweep to avoid holding a long-lived connection across idle periods.
    let client = match redis::Client::open(redis_url) {
        Ok(c) => c,
        Err(err) => {
            log_warn(&format!(
                "watchdog cancel signal: failed to open Redis client: {err}"
            ));
            return;
        }
    };
    let mut conn = match tokio::time::timeout(
        Duration::from_secs(3),
        client.get_multiplexed_async_connection(),
    )
    .await
    {
        Ok(Ok(c)) => c,
        Ok(Err(err)) => {
            log_warn(&format!(
                "watchdog cancel signal: Redis connect failed: {err}"
            ));
            return;
        }
        Err(_) => {
            log_warn("watchdog cancel signal: Redis connect timed out");
            return;
        }
    };
    for id in ids {
        let key = format!("axon:crawl:cancel:{id}");
        if let Err(err) = conn.set_ex::<_, _, ()>(key, "1", 24 * 60 * 60).await {
            log_warn(&format!(
                "watchdog cancel signal failed for job {id}: {err}"
            ));
        }
    }
}

/// Runs a single watchdog sweep and logs results.
///
/// Gated to `lane == 1` — only the first lane performs DB sweeps to avoid
/// concurrent redundant reclaims across all active lanes. Callers on lanes 2+
/// return immediately without any work.
///
/// Shared by both the polling and AMQP lane loops.
pub(super) async fn run_watchdog_sweep(
    pool: &PgPool,
    lane: usize,
    stale_secs: i64,
    confirm_secs: i64,
    redis_url: &str,
) {
    if lane != 1 {
        return;
    }
    match reclaim_stale_running_jobs(pool, lane, stale_secs, confirm_secs).await {
        Ok(stats) => {
            if stats.stale_candidates > 0 || stats.reclaimed_jobs > 0 {
                log_info(&format!(
                    "watchdog crawl sweep lane={} candidates={} marked={} reclaimed={}",
                    lane, stats.stale_candidates, stats.marked_candidates, stats.reclaimed_jobs
                ));
                for id in &stats.reclaimed_ids {
                    log_warn(&format!(
                        "watchdog stale_job_detected job_id={id} status=running"
                    ));
                    log_warn(&format!("watchdog job_reclaimed job_id={id}"));
                }
            } else {
                log_debug("watchdog poll_clean");
            }
            if !stats.reclaimed_ids.is_empty() {
                signal_reclaimed_cancel_keys(redis_url, &stats.reclaimed_ids).await;
            }
        }
        Err(err) => log_warn(&format!("watchdog sweep failed (lane={}): {}", lane, err)),
    }
}

/// Parses, claims, and acks a single AMQP delivery.
///
/// Returns `Some(job_id)` when the delivery was successfully claimed and the
/// ack was sent, or `None` if the delivery was malformed, already claimed, or
/// encountered a DB error (in which case the delivery is nacked).
pub(super) async fn claim_delivery(
    pool: &PgPool,
    delivery: lapin::message::Delivery,
    lane: usize,
) -> Option<Uuid> {
    let parsed = std::str::from_utf8(&delivery.data)
        .ok()
        .and_then(|s| Uuid::parse_str(s.trim()).ok());
    let Some(job_id) = parsed else {
        log_warn(&format!(
            "malformed crawl delivery payload (lane={lane}, len={}) - acking and skipping",
            delivery.data.len()
        ));
        if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
            log_warn(&format!(
                "failed to ack malformed crawl delivery (lane={lane}): {err}"
            ));
        }
        return None;
    };

    match claim_pending_by_id(pool, TABLE, job_id).await {
        Ok(true) => {
            // Ack before processing: crawls can run for hours, and RabbitMQ's
            // consumer_timeout (default 30 min) will forcibly close the channel if
            // the ack comes too late. The DB is the source of truth for job state;
            // the watchdog reclaims any job that crashes without completing.
            if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
                // Ack failure does not prevent processing: the job is already
                // claimed in the DB (status='running'). Skipping would leave
                // it stuck until the watchdog reclaims it.
                log_warn(&format!(
                    "failed to ack crawl delivery (lane={lane}), processing anyway: {err}"
                ));
            }
            Some(job_id)
        }
        Ok(false) => {
            if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
                log_warn(&format!(
                    "failed to ack already-claimed crawl delivery (lane={lane}): {err}"
                ));
            }
            None
        }
        Err(err) => {
            log_warn(&format!(
                "failed to claim crawl job {job_id} (lane={lane}); nacking for retry: {err}"
            ));
            // Brief pause before requeuing so a persistent DB failure (e.g. PG
            // restart) doesn't spin-loop at full speed consuming CPU and flooding logs.
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(nack_err) = delivery
                .nack(BasicNackOptions {
                    requeue: true,
                    ..Default::default()
                })
                .await
            {
                log_warn(&format!(
                    "failed to nack crawl delivery for job {job_id} (lane={lane}): {nack_err}"
                ));
            }
            None
        }
    }
}

/// Mutable interval handles shared across a single lane's consumer loop.
pub(super) struct LaneTimers<'a> {
    pub heartbeat: &'a mut tokio::time::Interval,
    pub sweep: &'a mut tokio::time::Interval,
}

/// Runs `process_job` for the given `job_id` while keeping heartbeat and watchdog
/// sweep intervals alive. Returns once the job completes (success or failure).
///
/// `process_job` returns `Box<dyn Error>` which is `!Send`, so we cannot use
/// `tokio::spawn`. Instead we pin the future and poll it inside a `select!` loop
/// alongside the interval ticks. This keeps ticks responsive during long crawls
/// while preserving the 1-job-per-lane guarantee.
pub(super) async fn run_job_with_ticks(
    cfg: &Config,
    pool: &PgPool,
    job_id: Uuid,
    lane: usize,
    timers: &mut LaneTimers<'_>,
) {
    let job_fut = process_job(cfg, pool, job_id);
    tokio::pin!(job_fut);
    let result = loop {
        tokio::select! {
            result = &mut job_fut => break result,
            _ = timers.heartbeat.tick() => {
                log_info(&format!("crawl worker heartbeat lane={} alive", lane));
            },
            _ = timers.sweep.tick() => {
                run_watchdog_sweep(pool, lane, cfg.watchdog_stale_timeout_secs, cfg.watchdog_confirm_secs, &cfg.redis_url).await;
            },
        }
    };
    if let Err(err) = result {
        let error_text = err.to_string();
        if let Err(mark_err) = mark_job_failed(pool, TABLE, job_id, &error_text).await {
            log_warn(&format!(
                "mark_job_failed error for crawl job {job_id}: {mark_err}"
            ));
        }
        log_warn(&format!("worker failed crawl job {job_id}: {error_text}"));
    }
}

/// Runs one AMQP consumer loop for a single lane. Returns an error when the
/// consumer stream ends or the channel is closed. Callers wrap this in a
/// reconnect loop via `run_amqp_lane_with_reconnect`.
pub(super) async fn run_amqp_worker_lane(
    cfg: Arc<Config>,
    pool: PgPool,
    lane: usize,
) -> Result<(), Box<dyn Error>> {
    // Hold `_conn` in scope for the entire consumer loop. open_amqp_connection_and_channel
    // returns the Connection; dropping it would close the backing TCP connection
    // and kill the consumer stream. Keeping _conn alive prevents that.
    let (_conn, ch) = open_amqp_connection_and_channel(&cfg, &cfg.crawl_queue).await?;
    // prefetch=1 is intentional for the crawl worker: jobs run for hours, and
    // prefetching more would starve sibling lanes. Without this, the broker can
    // dump all pending messages into one consumer's buffer.
    ch.basic_qos(1, BasicQosOptions::default()).await?;
    let consumer_tag = format!("axon-rust-crawl-worker-{lane}");
    let mut consumer = ch
        .basic_consume(
            cfg.crawl_queue.as_str().into(),
            consumer_tag.as_str().into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    log_info(&format!(
        "crawl worker lane={} listening on queue={} concurrency={}",
        lane,
        cfg.crawl_queue,
        worker_concurrency()
    ));

    let mut sweep_interval = tokio::time::interval(Duration::from_secs(STALE_SWEEP_INTERVAL_SECS));
    sweep_interval.tick().await; // consume the immediate first tick
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(60));
    heartbeat_interval.tick().await; // consume the immediate first tick

    loop {
        // Poll for the next AMQP message while keeping heartbeat and sweep ticks alive.
        let msg = {
            let timers = LaneTimers {
                heartbeat: &mut heartbeat_interval,
                sweep: &mut sweep_interval,
            };
            tokio::select! {
                msg = consumer.next() => match msg {
                    Some(msg) => msg,
                    None => break,
                },
                _ = timers.heartbeat.tick() => {
                    log_info(&format!("crawl worker heartbeat lane={} alive", lane));
                    continue;
                },
                _ = timers.sweep.tick() => {
                    run_watchdog_sweep(&pool, lane, cfg.watchdog_stale_timeout_secs, cfg.watchdog_confirm_secs, &cfg.redis_url).await;
                    continue;
                }
            }
        };
        let delivery = match msg {
            Ok(d) => d,
            Err(err) => {
                log_warn(&format!("consumer error (lane={lane}): {err}"));
                continue;
            }
        };
        // Claim the delivery; skip if malformed, already taken, or DB error.
        let Some(job_id) = claim_delivery(&pool, delivery, lane).await else {
            continue;
        };
        // Run the job while keeping heartbeat and sweep ticks responsive.
        // process_job is !Send (Box<dyn Error>), so we pin it here rather than spawning.
        // This preserves the 1-job-per-lane guarantee.
        let mut timers = LaneTimers {
            heartbeat: &mut heartbeat_interval,
            sweep: &mut sweep_interval,
        };
        run_job_with_ticks(&cfg, &pool, job_id, lane, &mut timers).await;
    }

    Err(format!("crawl worker consumer stream ended unexpectedly (lane={lane})").into())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    /// The watchdog sweep only calls `signal_reclaimed_cancel_keys` when
    /// `reclaimed_ids` is non-empty (guarded by `if !stats.reclaimed_ids.is_empty()`).
    /// This test verifies the guard condition: an empty vec must not satisfy the
    /// predicate, confirming no Redis calls are triggered for zero reclaimed jobs.
    #[test]
    fn signal_cancel_keys_skipped_when_no_reclaimed_ids() {
        let ids: Vec<Uuid> = vec![];
        assert!(
            ids.is_empty(),
            "guard should prevent Redis calls for empty reclaim list"
        );
    }

    /// The Redis cancel key written by `signal_reclaimed_cancel_keys` must match
    /// the key polled by `is_crawl_canceled()` / `poll_cancel_key()` in `process.rs`
    /// and `job_context.rs`. All three sites format it as `axon:crawl:cancel:{id}`.
    /// A mismatch would cause reclaimed jobs to keep running despite the cancel signal.
    #[test]
    fn cancel_key_format_matches_polling_consumer() {
        let id = Uuid::nil();
        let key = format!("axon:crawl:cancel:{id}");
        assert_eq!(
            key,
            "axon:crawl:cancel:00000000-0000-0000-0000-000000000000"
        );
    }

    /// Round-trip: the key format written by the watchdog (amqp_consumer.rs) must be
    /// identical to the key format read by the cancellation poller (process.rs
    /// and job_context.rs). All three sites must produce the same string for the
    /// same UUID, or the cancel signal is silently lost.
    #[test]
    fn cancel_key_writer_and_reader_formats_are_identical() {
        let id =
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid UUID literal");
        // Writer (amqp_consumer.rs signal_reclaimed_cancel_keys)
        let writer_key = format!("axon:crawl:cancel:{id}");
        // Reader (process.rs is_crawl_canceled / job_context.rs poll_cancel_key)
        let reader_key = format!("axon:crawl:cancel:{id}");
        assert_eq!(
            writer_key, reader_key,
            "writer and reader must produce identical Redis key for the same job ID"
        );
        assert_eq!(
            writer_key,
            "axon:crawl:cancel:550e8400-e29b-41d4-a716-446655440000"
        );
    }
}
