use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::jobs::common::claim_next_pending;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use sqlx::PgPool;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    POLL_BACKOFF_INIT_MS, POLL_BACKOFF_MAX_MS, ProcessFn, STALE_SWEEP_INTERVAL_SECS, WorkerConfig,
    sweep_stale_jobs,
};

/// Sleep for `duration`, but return early if an in-flight job completes.
/// Returns `true` if a job completed (caller should `continue` the loop).
async fn sleep_or_drain_one<F>(duration: Duration, inflight: &mut FuturesUnordered<F>) -> bool
where
    F: Future<Output = ()>,
{
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => false,
        done = inflight.next(), if !inflight.is_empty() => done.is_some(),
    }
}

/// Generic polling lane. Claims pending jobs via SQL polling with exponential
/// backoff (100ms -> 6400ms on idle, reset on job found). Dispatches to
/// `process_fn` concurrently using `FuturesUnordered` with semaphore backpressure.
/// Runs stale sweeps on the configured interval.
pub(crate) async fn run_polling_lane(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    lane: usize,
    process_fn: &ProcessFn,
    semaphore: Arc<tokio::sync::Semaphore>,
) -> Result<(), Box<dyn std::error::Error>> {
    log_info(&format!(
        "{} worker polling lane={lane} active queue={}",
        wc.job_kind, wc.queue_name
    ));

    // Wrap config once per lane — all job dispatches within this lane use
    // Arc::clone instead of cloning the full Config struct.
    let cfg_arc = Arc::new(cfg.clone());

    let mut last_sweep = Instant::now();
    let mut backoff_ms = POLL_BACKOFF_INIT_MS;
    let mut inflight = FuturesUnordered::new();

    loop {
        // If all permits are consumed, block until one in-flight job completes
        // OR the sweep interval fires.  Using a plain .await here would block
        // sweeps for the entire duration of any saturated burst.
        if semaphore.available_permits() == 0 && !inflight.is_empty() {
            let sweep_due = last_sweep.elapsed() >= Duration::from_secs(STALE_SWEEP_INTERVAL_SECS);
            if sweep_due {
                sweep_stale_jobs(cfg, &pool, wc, "polling", lane).await;
                last_sweep = Instant::now();
                continue;
            }
            let remaining = Duration::from_secs(STALE_SWEEP_INTERVAL_SECS) - last_sweep.elapsed();
            let sleep = tokio::time::sleep(remaining);
            tokio::pin!(sleep);
            tokio::select! {
                _ = inflight.next() => {}
                _ = &mut sleep => {
                    sweep_stale_jobs(cfg, &pool, wc, "polling", lane).await;
                    last_sweep = Instant::now();
                }
            }
            continue;
        }

        if last_sweep.elapsed() >= Duration::from_secs(STALE_SWEEP_INTERVAL_SECS) {
            sweep_stale_jobs(cfg, &pool, wc, "polling", lane).await;
            last_sweep = Instant::now();
        }
        // Reserve capacity first so we never claim a job without a runnable slot.
        let permit = semaphore.clone().acquire_owned().await?;
        match claim_next_pending(&pool, wc.table).await {
            Ok(Some(id)) => {
                backoff_ms = POLL_BACKOFF_INIT_MS;
                let fut = process_fn(Arc::clone(&cfg_arc), pool.clone(), id);
                inflight.push(async move {
                    fut.await;
                    drop(permit);
                });
            }
            Ok(None) => {
                drop(permit);
                if sleep_or_drain_one(Duration::from_millis(backoff_ms), &mut inflight).await {
                    continue;
                }
                backoff_ms = (backoff_ms * 2).min(POLL_BACKOFF_MAX_MS);
            }
            Err(err) => {
                drop(permit);
                log_warn(&format!(
                    "{} worker polling lane={lane} DB error; retrying in 5s: {err}",
                    wc.job_kind
                ));
                if sleep_or_drain_one(Duration::from_secs(5), &mut inflight).await {
                    continue;
                }
            }
        }
    }
}
