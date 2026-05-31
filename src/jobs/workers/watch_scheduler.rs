//! In-process watch scheduler.
//!
//! A single periodic loop that fires recurring watch definitions. Each tick
//! atomically leases every enabled watch whose `next_run_at` has passed (see
//! [`lease_due_watches`]) and runs each leased watch via
//! [`run_watch_now_with_pool`], which records a run, reschedules `next_run_at`
//! to `now + every_seconds`, and clears the lease. Leasing is what makes the
//! loop safe to run alongside other processes and across crashes: an in-flight
//! watch holds a lease until it finishes, and a crash leaves the lease in place
//! only until it expires, after which the next sweep (or the startup
//! `reclaim_stale_watch_leases` call) frees it for re-run.
//!
//! Tuning (read once at spawn, mirroring `AXON_JOB_WAIT_TIMEOUT_SECS` in
//! `backend.rs`):
//! - `AXON_WATCH_TICK_SECS` — seconds between sweeps (default 15, min 1).
//! - `AXON_WATCH_LEASE_SECS` — lease TTL; must exceed a single run's wall time
//!   so a long run is never double-fired (default 300, min 1).

use crate::core::config::Config;
use crate::jobs::store::now_ms;
use crate::jobs::watch::{lease_due_watches, run_watch_now_with_pool};
use sqlx::SqlitePool;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const DEFAULT_TICK_SECS: u64 = 15;
const DEFAULT_LEASE_SECS: i64 = 300;
/// Cap watches leased per tick so one sweep can't spawn an unbounded number of
/// concurrent runs. The lease keeps any watch left over due for the next tick.
const LEASE_BATCH_LIMIT: i64 = 32;

fn parse_tick_secs(raw: Option<String>) -> u64 {
    raw.and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs >= 1)
        .unwrap_or(DEFAULT_TICK_SECS)
}

fn parse_lease_secs(raw: Option<String>) -> i64 {
    raw.and_then(|raw| raw.parse::<i64>().ok())
        .filter(|secs| *secs >= 1)
        .unwrap_or(DEFAULT_LEASE_SECS)
}

fn tick_interval() -> Duration {
    Duration::from_secs(parse_tick_secs(std::env::var("AXON_WATCH_TICK_SECS").ok()))
}

fn lease_ttl_ms() -> i64 {
    parse_lease_secs(std::env::var("AXON_WATCH_LEASE_SECS").ok()) * 1_000
}

/// Run one sweep: lease due watches and spawn a detached run for each.
///
/// Each run is spawned rather than awaited inline so a slow scrape never stalls
/// the sweep or delays other due watches; the lease prevents the next tick from
/// re-firing a watch whose run is still in flight.
async fn sweep_due_watches(
    pool: &Arc<SqlitePool>,
    cfg: &Arc<Config>,
    lease_ttl_ms: i64,
) -> Result<usize, Box<dyn Error>> {
    let due = lease_due_watches(pool, now_ms(), lease_ttl_ms, LEASE_BATCH_LIMIT).await?;
    let count = due.len();
    for watch in due {
        let pool = Arc::clone(pool);
        let cfg = Arc::clone(cfg);
        tokio::spawn(async move {
            if let Err(err) = run_watch_now_with_pool(&cfg, &pool, &watch).await {
                tracing::warn!(watch_id = %watch.id, name = %watch.name, error = %err, "watch scheduler: run failed");
            }
        });
    }
    Ok(count)
}

/// Periodic scheduler loop. Spawned once by `spawn_workers`; exits on shutdown.
pub(super) async fn watch_scheduler_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    shutdown: CancellationToken,
) {
    let lease_ttl = lease_ttl_ms();
    let mut ticker = tokio::time::interval(tick_interval());
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the immediate first tick — startup lease reclaim already ran in
    // SqliteJobBackend init, and we want the first sweep one interval out.
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                match sweep_due_watches(&pool, &cfg, lease_ttl).await {
                    Ok(fired) if fired > 0 => {
                        tracing::debug!(fired, "watch scheduler: dispatched due watches");
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(error = %err, "watch scheduler: sweep failed");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_scheduler_tests.rs"]
mod tests;
