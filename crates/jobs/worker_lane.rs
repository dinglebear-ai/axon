mod amqp;
mod delivery;
mod poll;

use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::jobs::common::{
    JobTable, batch_enqueue_jobs, open_amqp_connection_and_channel, reclaim_stale_running_jobs,
};
use crate::crates::jobs::status::JobStatus;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const STALE_SWEEP_INTERVAL_SECS: u64 = 30;

/// Resolve worker lane count: env-var override takes priority; falls back to a
/// CPU-based default clamped to `[cpu_min, cpu_max]`.
///
/// Example: `resolve_lane_count("AXON_EMBED_LANES", 2, 32)` → number of logical
/// CPUs (min 2, max 32), overridable at runtime via `AXON_EMBED_LANES=N`.
pub(crate) fn resolve_lane_count(env_var: &str, cpu_min: usize, cpu_max: usize) -> usize {
    let cpu_default = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(cpu_min, cpu_max);
    std::env::var(env_var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(cpu_default)
}

/// Polling backoff constants (milliseconds).
pub(crate) const POLL_BACKOFF_INIT_MS: u64 = 100;
pub(crate) const POLL_BACKOFF_MAX_MS: u64 = 6400;

/// AMQP reconnect backoff: starts at 2s, doubles on each consecutive failure,
/// capped at 60s.  Reset to the initial value on a successful connection.
const AMQP_RECONNECT_INIT_SECS: u64 = 2;
const AMQP_RECONNECT_MAX_SECS: u64 = 60;

/// A boxed async function that processes a single claimed job.
/// It must handle its own error logging/marking internally (returns `()`).
pub(crate) type ProcessFn =
    Arc<dyn Fn(Config, PgPool, uuid::Uuid) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync>;

/// Configuration for a generic worker.
pub(crate) struct WorkerConfig {
    pub table: JobTable,
    pub queue_name: String,
    pub job_kind: &'static str,
    pub consumer_tag_prefix: &'static str,
    pub lane_count: usize,
}

/// Validate that the critical infrastructure environment variables are present
/// before the worker attempts any network connections.
///
/// Required variables:
/// - Postgres: `AXON_PG_URL`
/// - Redis:    `AXON_REDIS_URL`
/// - AMQP:     `AXON_AMQP_URL`
///
/// Note: this checks for the presence of at least one variable per service, not
/// the validity of the URL. Connection errors are still reported at connect time.
pub(crate) fn validate_worker_env_vars() -> Result<(), String> {
    let mut missing: Vec<&'static str> = Vec::with_capacity(3);

    if std::env::var("AXON_PG_URL").is_err() {
        missing.push("AXON_PG_URL");
    }
    if std::env::var("AXON_REDIS_URL").is_err() {
        missing.push("AXON_REDIS_URL");
    }
    if std::env::var("AXON_AMQP_URL").is_err() {
        missing.push("AXON_AMQP_URL");
    }

    if !missing.is_empty() {
        return Err(format!(
            "worker startup error: the following required environment variables are not set:\n  {}\n\
             Set them in your environment or .env file before starting the worker.",
            missing.join("\n  ")
        ));
    }
    Ok(())
}

/// Run the stale-job sweep and log results.
pub(crate) async fn sweep_stale_jobs(
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
    source: &str,
    lane: usize,
) {
    match reclaim_stale_running_jobs(
        pool,
        wc.table,
        wc.job_kind,
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        source,
    )
    .await
    {
        Ok(stats) => {
            if stats.stale_candidates > 0 || stats.reclaimed_jobs > 0 {
                log_info(&format!(
                    "watchdog {} sweep lane={} candidates={} marked={} reclaimed={}",
                    wc.job_kind,
                    lane,
                    stats.stale_candidates,
                    stats.marked_candidates,
                    stats.reclaimed_jobs
                ));
                for id in &stats.reclaimed_ids {
                    log_warn(&format!(
                        "watchdog stale_{}_job job_id={id} reclaimed=true",
                        wc.job_kind
                    ));
                }
            } else {
                log_debug(&format!(
                    "watchdog poll_clean worker={} lane={lane}",
                    wc.job_kind
                ));
            }
        }
        Err(e) => {
            log_warn(&format!(
                "watchdog {} sweep failed (lane={lane}): {e}",
                wc.job_kind
            ));
        }
    }
}

async fn probe_amqp_available(cfg: &Config, wc: &WorkerConfig) -> bool {
    match open_amqp_connection_and_channel(cfg, &wc.queue_name).await {
        Ok((conn, ch)) => {
            if let Err(e) = ch.close(0, "probe".into()).await {
                log_debug(&format!(
                    "amqp ch_close failed queue={} error={e}",
                    wc.queue_name
                ));
            }
            if let Err(e) = conn.close(200, "probe".into()).await {
                log_debug(&format!(
                    "amqp conn_close failed queue={} error={e}",
                    wc.queue_name
                ));
            }
            true
        }
        Err(e) => {
            log_warn(&format!(
                "{} worker: AMQP probe failed ({}), falling back to polling: {e}",
                wc.job_kind, wc.queue_name
            ));
            false
        }
    }
}

async fn run_amqp_reconnect_loop(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    process_fn: &ProcessFn,
    semaphore: Arc<tokio::sync::Semaphore>,
) {
    let mut reconnect_delay_secs = AMQP_RECONNECT_INIT_SECS;
    loop {
        let lane_start = tokio::time::Instant::now();
        let futs: Vec<_> = (1..=wc.lane_count)
            .map(|lane| {
                amqp::run_amqp_lane(cfg, pool.clone(), wc, lane, process_fn, semaphore.clone())
            })
            .collect();
        let results = futures_util::future::join_all(futs).await;
        let ran_for_secs = lane_start.elapsed().as_secs();
        let mut any_unexpected = false;
        for result in results {
            if let Err(err) = result {
                log_warn(&format!(
                    "{} worker lane terminated unexpectedly: {err}",
                    wc.job_kind
                ));
                any_unexpected = true;
            }
        }
        if any_unexpected {
            if ran_for_secs >= AMQP_RECONNECT_MAX_SECS {
                reconnect_delay_secs = AMQP_RECONNECT_INIT_SECS;
            }
            log_warn(&format!(
                "{} worker restarting AMQP lanes in {reconnect_delay_secs}s",
                wc.job_kind
            ));
            tokio::time::sleep(Duration::from_secs(reconnect_delay_secs)).await;
            reconnect_delay_secs = (reconnect_delay_secs * 2).min(AMQP_RECONNECT_MAX_SECS);
        }
    }
}

async fn run_polling_lanes(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    process_fn: &ProcessFn,
    semaphore: Arc<tokio::sync::Semaphore>,
) -> Result<(), Box<dyn std::error::Error>> {
    log_warn(&format!(
        "amqp unavailable; running {} worker in postgres polling mode",
        wc.job_kind
    ));
    let futs: Vec<_> = (1..=wc.lane_count)
        .map(|lane| {
            poll::run_polling_lane(cfg, pool.clone(), wc, lane, process_fn, semaphore.clone())
        })
        .collect();
    let results = futures_util::future::join_all(futs).await;
    for result in results {
        result?;
    }
    Ok(())
}

/// Generic top-level worker: startup sweep, probe AMQP, then run `lane_count` lanes
/// (AMQP or polling fallback) using `futures_util::future::join_all` for dynamic concurrency.
///
/// A shared `Semaphore` limits total in-flight spawned tasks to `lane_count`.
///
/// How long a pending job must sit unprocessed before we consider it orphaned.
/// Using `watchdog_stale_timeout_secs` as a proxy: if a pending job is older
/// than the stale threshold it was never picked up after a broker restart.
fn orphaned_pending_threshold_secs(stale_timeout_secs: i64) -> i32 {
    stale_timeout_secs.max(60).min(i32::MAX as i64) as i32
}

fn orphaned_pending_select_query(table: JobTable) -> String {
    format!(
        "SELECT id FROM {} WHERE status = $1 AND created_at < NOW() - make_interval(secs => $2)",
        table.as_str()
    )
}

/// At AMQP worker startup, re-enqueue any jobs that are stuck in `pending`
/// state longer than the stale threshold. These are jobs that were enqueued
/// before a broker restart — the AMQP message was lost but the DB row remains.
async fn reenqueue_orphaned_pending_jobs(
    cfg: &Config,
    pool: &PgPool,
    wc: &WorkerConfig,
) -> Result<u64, Box<dyn std::error::Error>> {
    let threshold = orphaned_pending_threshold_secs(cfg.watchdog_stale_timeout_secs);
    let query = orphaned_pending_select_query(wc.table);
    let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&query)
        .bind(JobStatus::Pending.as_str())
        .bind(threshold)
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let ids: Vec<uuid::Uuid> = rows.into_iter().map(|(id,)| id).collect();
    let count = ids.len() as u64;
    // Concurrency safety: between the SELECT above and the AMQP publish below, a job may
    // transition from `pending` to `running` if a worker lane claims it. This is safe because
    // `claim_next_pending` uses a `WHERE status = 'pending'` guard — a running job will not
    // be claimed again. The AMQP message will be published but the consumer will find no
    // claimable row and nack/discard it.
    batch_enqueue_jobs(cfg, &wc.queue_name, &ids).await?;
    Ok(count)
}

/// Callers must call `make_pool` and `ensure_schema` before invoking this.
pub(crate) async fn run_job_worker(
    cfg: &Config,
    pool: PgPool,
    wc: &WorkerConfig,
    process_fn: ProcessFn,
) -> Result<(), Box<dyn std::error::Error>> {
    if wc.lane_count == 0 {
        return Err(format!("{} worker: lane_count must be >= 1", wc.job_kind).into());
    }

    sweep_stale_jobs(cfg, &pool, wc, "startup", 0).await;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(wc.lane_count));

    let amqp_available = probe_amqp_available(cfg, wc).await;

    if amqp_available {
        match reenqueue_orphaned_pending_jobs(cfg, &pool, wc).await {
            Ok(0) => {}
            Ok(n) => log_info(&format!(
                "{} worker: re-enqueued {n} orphaned pending job(s) from before broker restart",
                wc.job_kind
            )),
            Err(err) => log_warn(&format!(
                "{} worker: orphaned pending re-enqueue failed (non-fatal): {err}",
                wc.job_kind
            )),
        }
        run_amqp_reconnect_loop(cfg, pool.clone(), wc, &process_fn, semaphore.clone()).await;
    }

    // Polling fallback: AMQP was unavailable at startup so we fall back to
    // SQL polling.  Unlike the AMQP path, the polling path has no internal
    // reconnect loop — a Postgres restart will kill the worker permanently.
    // Recovery is intentionally delegated to the s6 process supervisor, which
    // will restart the worker binary automatically.  Do NOT add a reconnect
    // loop here without carefully considering the implications of concurrent
    // polling restarts stomping on each other's state.
    run_polling_lanes(cfg, pool, wc, &process_fn, semaphore).await
}

#[cfg(test)]
mod tests;
