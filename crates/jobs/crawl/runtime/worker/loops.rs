//! Crawl worker loop orchestration.
//!
//! # Why crawl doesn't use the generic worker_lane module
//!
//! `worker_lane.rs` (module root file) is the generic AMQP/polling lane runtime shared by embed,
//! extract, and refresh workers. The crawl worker uses its own loop because
//! `spider.rs` futures are `!Send` — they cannot be spawned with `tokio::spawn`
//! or moved across thread boundaries. Instead, crawl pins futures via `tokio::pin!`
//! and polls them inside a `select!` loop. This preserves 1-job-per-lane
//! semantics while keeping the non-Send future alive on the same thread.

use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::jobs::common::{
    claim_next_pending, make_pool, mark_job_failed, open_amqp_connection_and_channel,
};
use crate::crates::jobs::worker_lane::validate_worker_env_vars;
use sqlx::PgPool;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::{
    STALE_SWEEP_INTERVAL_SECS, TABLE, ensure_schema, reenqueue_orphaned_pending_jobs,
    worker_concurrency,
};
use super::amqp_consumer::{reclaim_stale_running_jobs, run_amqp_worker_lane, run_watchdog_sweep};
use super::process::process_job;

async fn run_worker_polling_loop(cfg: &Config, pool: &PgPool) -> Result<(), Box<dyn Error>> {
    log_warn("amqp unavailable; running crawl worker in postgres polling mode");
    if worker_concurrency() <= 1 {
        return run_worker_polling_lane(cfg, pool, 1).await;
    }
    // Use join_all so a lane failure does not abruptly cancel sibling lanes
    // mid-job (which would leave jobs stuck in 'running' until the watchdog reclaims them).
    let results = futures_util::future::join_all(
        (1..=worker_concurrency()).map(|lane| run_worker_polling_lane(cfg, pool, lane)),
    )
    .await;
    for r in results {
        r?;
    }
    Ok(())
}

async fn run_worker_polling_lane(
    cfg: &Config,
    pool: &PgPool,
    lane: usize,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "crawl worker polling lane={} active queue={}",
        lane, cfg.crawl_queue
    ));
    let mut last_sweep = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut backoff_ms: u64 = 100;
    loop {
        if last_heartbeat.elapsed() >= Duration::from_secs(60) {
            log_info(&format!("crawl worker heartbeat lane={} alive", lane));
            last_heartbeat = Instant::now();
        }
        if last_sweep.elapsed() >= Duration::from_secs(STALE_SWEEP_INTERVAL_SECS) {
            run_watchdog_sweep(
                pool,
                lane,
                cfg.watchdog_stale_timeout_secs,
                cfg.watchdog_confirm_secs,
                &cfg.redis_url,
            )
            .await;
            last_sweep = Instant::now();
        }
        if let Some(job_id) = claim_next_pending(pool, TABLE).await? {
            backoff_ms = 100; // Reset backoff on successful claim
            if let Err(err) = process_job(cfg, pool, job_id).await {
                let error_text = err.to_string();
                if let Err(mark_err) = mark_job_failed(pool, TABLE, job_id, &error_text).await {
                    log_warn(&format!(
                        "mark_job_failed error for crawl job {job_id}: {mark_err}"
                    ));
                }
                log_warn(&format!("worker failed crawl job {job_id}: {error_text}"));
            }
        } else {
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(6400);
        }
    }
}

/// Initial reconnect backoff in seconds. Doubles on each attempt, capped at 60s.
const RECONNECT_BACKOFF_INITIAL_SECS: u64 = 2;
const RECONNECT_BACKOFF_MAX_SECS: u64 = 60;

/// Wraps `run_amqp_worker_lane` in an infinite reconnect loop with exponential
/// backoff. When the channel dies (AMQP consumer_timeout, broker restart, etc.),
/// the current job completes normally (it holds no AMQP channel reference), then
/// the lane reconnects and resumes. This function never returns; callers use
/// `tokio::join!` to run multiple lanes concurrently.
async fn run_amqp_lane_with_reconnect(cfg: Arc<Config>, pool: PgPool, lane: usize) {
    let mut backoff_secs = RECONNECT_BACKOFF_INITIAL_SECS;
    loop {
        match run_amqp_worker_lane(Arc::clone(&cfg), pool.clone(), lane).await {
            Ok(()) => {
                // run_amqp_worker_lane only returns Ok if the consumer stream
                // ended cleanly, which shouldn't happen in normal operation.
                // Reset backoff so the next reconnect starts from the initial
                // delay rather than an inflated cap from previous failures.
                backoff_secs = RECONNECT_BACKOFF_INITIAL_SECS;
                log_warn(&format!(
                    "crawl worker lane={lane} AMQP loop exited cleanly; reconnecting immediately"
                ));
                // No sleep on clean exit — reconnect immediately.
            }
            Err(err) => {
                log_warn(&format!(
                    "crawl worker lane={lane} AMQP error: {err}; reconnecting in {backoff_secs}s"
                ));
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(RECONNECT_BACKOFF_MAX_SECS);
            }
        }
        log_info(&format!(
            "crawl worker lane={lane} attempting AMQP reconnect"
        ));
    }
}

pub(crate) async fn run_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Validate required environment variables before attempting any connections.
    if let Err(msg) = validate_worker_env_vars() {
        return Err(msg.into());
    }

    log_info(&format!(
        "worker_start worker=crawl queue={} render_mode={:?} max_pages={}",
        cfg.crawl_queue, cfg.render_mode, cfg.max_pages
    ));

    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    match reclaim_stale_running_jobs(
        &pool,
        0,
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
    )
    .await
    {
        Ok(stats) => {
            if stats.stale_candidates > 0 || stats.reclaimed_jobs > 0 {
                log_info(&format!(
                    "watchdog crawl startup sweep candidates={} marked={} reclaimed={}",
                    stats.stale_candidates, stats.marked_candidates, stats.reclaimed_jobs
                ));
            }
        }
        Err(err) => log_warn(&format!("watchdog crawl startup sweep failed: {err}")),
    }

    // Probe AMQP connectivity with a short-lived connection+channel pair.
    // Close both explicitly so RabbitMQ doesn't accumulate orphaned channels.
    // Each lane opens its own long-lived connection for its consumer loop.
    let amqp_available = match open_amqp_connection_and_channel(cfg, &cfg.crawl_queue).await {
        Ok((conn, ch)) => {
            if let Err(e) = ch.close(0, "probe".into()).await {
                log_debug(&format!(
                    "amqp ch_close failed queue={} error={e}",
                    cfg.crawl_queue
                ));
            }
            if let Err(e) = conn.close(200, "probe".into()).await {
                log_debug(&format!(
                    "amqp conn_close failed queue={} error={e}",
                    cfg.crawl_queue
                ));
            }
            true
        }
        Err(e) => {
            log_warn(&format!(
                "crawl worker: AMQP probe failed ({}), falling back to polling: {e}",
                cfg.crawl_queue
            ));
            false
        }
    };
    if !amqp_available {
        return run_worker_polling_loop(cfg, &pool).await;
    }

    // Re-enqueue any pending jobs that survived a broker restart or were
    // submitted while no worker was running. Done after the AMQP probe so
    // we only publish when we know the broker is reachable.
    match reenqueue_orphaned_pending_jobs(cfg, &pool).await {
        Ok(0) => {}
        Ok(n) => log_info(&format!(
            "crawl worker startup: re-enqueued {n} orphaned pending job(s)"
        )),
        Err(err) => log_warn(&format!(
            "crawl worker startup: orphaned pending re-enqueue failed: {err}"
        )),
    }
    let cfg_arc = Arc::new(cfg.clone());
    if worker_concurrency() <= 1 {
        run_amqp_lane_with_reconnect(cfg_arc, pool, 1).await;
        return Ok(());
    }

    // Run all lanes concurrently. Each lane has its own reconnect loop, so a
    // channel death in one lane does not affect the others — each reconnects
    // independently. Lane independence is achieved by the reconnect loop, not
    // by separate OS threads: if one lane's channel dies it reconnects while
    // the others continue uninterrupted.
    //
    // Note: run_amqp_lane_with_reconnect is !Send (process_job uses Box<dyn Error>),
    // so tokio::spawn cannot be used here. join_all runs all lanes on the same
    // task, which is correct — each lane has its own reconnect guard.
    futures_util::future::join_all(
        (1..=worker_concurrency())
            .map(|lane| run_amqp_lane_with_reconnect(Arc::clone(&cfg_arc), pool.clone(), lane)),
    )
    .await;
    Ok(())
}
