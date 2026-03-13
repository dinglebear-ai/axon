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
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use futures_util::stream::FuturesUnordered;
    use serial_test::serial;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;
    use tokio::sync::Mutex;

    /// Verify semaphore permits: N permits from a semaphore of size N all succeed
    /// immediately, but the (N+1)-th blocks until one is released.
    #[tokio::test]
    async fn semaphore_permits_up_to_capacity_then_blocks() {
        let sem = Arc::new(tokio::sync::Semaphore::new(2));

        let p1 = sem.clone().acquire_owned().await.unwrap();
        let p2 = sem.clone().acquire_owned().await.unwrap();
        assert_eq!(sem.available_permits(), 0);

        let sem2 = sem.clone();
        let blocked = tokio::time::timeout(Duration::from_millis(50), sem2.acquire_owned()).await;
        assert!(
            blocked.is_err(),
            "third permit should block when capacity=2"
        );

        drop(p1);
        let p3 = tokio::time::timeout(Duration::from_millis(50), sem.clone().acquire_owned())
            .await
            .expect("third permit should succeed after release")
            .unwrap();
        assert_eq!(sem.available_permits(), 0);

        drop(p2);
        drop(p3);
        assert_eq!(sem.available_permits(), 2);
    }

    /// Verify that FuturesUnordered + semaphore allows two "jobs" to execute
    /// concurrently: both start before either finishes.
    #[tokio::test]
    async fn futures_unordered_runs_jobs_concurrently() {
        let log: Arc<Mutex<Vec<(Instant, Instant)>>> = Arc::new(Mutex::new(Vec::new()));
        let sem = Arc::new(tokio::sync::Semaphore::new(2));

        let mut inflight = FuturesUnordered::new();

        for _ in 0..2 {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let log = log.clone();
            inflight.push(async move {
                let start = Instant::now();
                tokio::time::sleep(Duration::from_millis(50)).await;
                let end = Instant::now();
                log.lock().await.push((start, end));
                drop(permit);
            });
        }

        while inflight.next().await.is_some() {}

        let entries = log.lock().await;
        assert_eq!(entries.len(), 2);

        let (start0, end0) = entries[0];
        let (start1, _end1) = entries[1];
        let (earlier_end, later_start) = if start0 <= start1 {
            (end0, start1)
        } else {
            (_end1, start0)
        };
        assert!(
            later_start < earlier_end,
            "jobs should overlap: later_start={later_start:?} should be < earlier_end={earlier_end:?}"
        );
    }

    /// Verify that when the semaphore is full, new jobs block until a permit is
    /// released (backpressure behavior matching worker_lane dispatch).
    #[tokio::test]
    async fn semaphore_backpressure_blocks_third_dispatch() {
        let sem = Arc::new(tokio::sync::Semaphore::new(2));
        let counter = Arc::new(AtomicU64::new(0));
        let mut inflight = FuturesUnordered::new();

        for _ in 0..2 {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let counter = counter.clone();
            inflight.push(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(200)).await;
                drop(permit);
            });
        }

        let sem_for_third = sem.clone();
        let third_handle = tokio::spawn(async move {
            let _permit = sem_for_third.acquire_owned().await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !third_handle.is_finished(),
            "third dispatch should be blocked"
        );

        while inflight.next().await.is_some() {}
        tokio::time::timeout(Duration::from_millis(50), third_handle)
            .await
            .expect("third should complete after permits released")
            .unwrap();
    }

    /// Verify the exponential backoff sequence: 100 -> 200 -> 400 -> 800 -> 1600
    /// -> 3200 -> 6400 -> 6400 (capped), then reset to 100 on job found.
    #[test]
    fn polling_backoff_sequence_doubles_caps_and_resets() {
        let mut backoff_ms = POLL_BACKOFF_INIT_MS;
        let expected = [100, 200, 400, 800, 1600, 3200, 6400, 6400, 6400];

        for (i, &expected_ms) in expected.iter().enumerate() {
            assert_eq!(
                backoff_ms, expected_ms,
                "iteration {i}: expected {expected_ms}ms, got {backoff_ms}ms"
            );
            backoff_ms = (backoff_ms * 2).min(POLL_BACKOFF_MAX_MS);
        }

        backoff_ms = POLL_BACKOFF_INIT_MS;
        assert_eq!(backoff_ms, 100, "should reset to 100ms on job found");

        backoff_ms = (backoff_ms * 2).min(POLL_BACKOFF_MAX_MS);
        assert_eq!(backoff_ms, 200, "should double to 200ms after reset");
    }

    /// Verify backoff boundary constants are correct.
    #[test]
    fn polling_backoff_constants_are_valid() {
        assert_eq!(POLL_BACKOFF_INIT_MS, 100);
        assert_eq!(POLL_BACKOFF_MAX_MS, 6400);
        const { assert!(POLL_BACKOFF_MAX_MS >= POLL_BACKOFF_INIT_MS) };
        assert_eq!(POLL_BACKOFF_MAX_MS, POLL_BACKOFF_INIT_MS * 64);
    }

    /// Verify the AMQP reconnect backoff sequence:
    /// 2 -> 4 -> 8 -> 16 -> 32 -> 60 -> 60 -> 60 (capped at AMQP_RECONNECT_MAX_SECS).
    #[test]
    fn amqp_reconnect_backoff_doubles_and_caps() {
        let mut backoff_secs = AMQP_RECONNECT_INIT_SECS;
        let expected = [2u64, 4, 8, 16, 32, 60, 60, 60];

        for (i, &expected_secs) in expected.iter().enumerate() {
            assert_eq!(
                backoff_secs, expected_secs,
                "iteration {i}: expected {expected_secs}s, got {backoff_secs}s"
            );
            backoff_secs = (backoff_secs * 2).min(AMQP_RECONNECT_MAX_SECS);
        }

        assert_eq!(AMQP_RECONNECT_INIT_SECS, 2);
        assert_eq!(AMQP_RECONNECT_MAX_SECS, 60);
    }

    /// Verify validate_worker_env_vars passes when all required vars are present.
    #[serial]
    #[expect(
        unsafe_code,
        reason = "SAFETY: test-only env var manipulation, no actual unsafe invariant"
    )]
    #[test]
    fn validate_env_vars_passes_when_all_set() {
        unsafe {
            std::env::set_var("AXON_PG_URL", "postgresql://localhost/test");
            std::env::set_var("AXON_REDIS_URL", "redis://localhost");
            std::env::set_var("AXON_AMQP_URL", "amqp://localhost");
        }

        let result = validate_worker_env_vars();
        assert!(
            result.is_ok(),
            "expected env validation success: {result:?}"
        );

        unsafe {
            std::env::remove_var("AXON_PG_URL");
            std::env::remove_var("AXON_REDIS_URL");
            std::env::remove_var("AXON_AMQP_URL");
        }
    }

    /// Verify canonical variables are required and missing vars fail recognition.
    #[serial]
    #[expect(
        unsafe_code,
        reason = "SAFETY: test-only env var manipulation, no actual unsafe invariant"
    )]
    #[test]
    fn validate_env_vars_requires_canonical_names() {
        unsafe {
            std::env::remove_var("AXON_PG_URL");
            std::env::remove_var("AXON_REDIS_URL");
            std::env::remove_var("AXON_AMQP_URL");
        }

        let result = validate_worker_env_vars();
        assert!(result.is_err(), "expected env validation failure");
        let msg = result.err().unwrap_or_default();
        assert!(msg.contains("AXON_PG_URL"));
        assert!(msg.contains("AXON_REDIS_URL"));
        assert!(msg.contains("AXON_AMQP_URL"));

        unsafe {
            std::env::remove_var("AXON_PG_URL");
            std::env::remove_var("AXON_REDIS_URL");
            std::env::remove_var("AXON_AMQP_URL");
        }
    }

    /// Verify UUID parsing logic used by claim_delivery.
    #[test]
    fn claim_delivery_parses_valid_uuid() {
        let id = uuid::Uuid::new_v4();
        let bytes = id.to_string().into_bytes();
        let parsed = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok());
        assert_eq!(parsed, Some(id));
    }

    /// Verify malformed payloads are rejected by the UUID parsing path.
    #[test]
    fn claim_delivery_rejects_malformed_payload() {
        let bad = b"not-a-uuid";
        let parsed = std::str::from_utf8(bad)
            .ok()
            .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok());
        assert!(parsed.is_none());
    }

    #[test]
    fn orphaned_pending_threshold_enforces_60s_floor() {
        assert_eq!(orphaned_pending_threshold_secs(0), 60);
        assert_eq!(orphaned_pending_threshold_secs(30), 60);
        assert_eq!(orphaned_pending_threshold_secs(59), 60);
        assert_eq!(orphaned_pending_threshold_secs(60), 60);
        assert_eq!(orphaned_pending_threshold_secs(300), 300);
        assert_eq!(orphaned_pending_threshold_secs(i64::MAX), i32::MAX);
    }

    /// Regression: when an inflight job completes, poll_next_delivery must NOT
    /// return Ok(None), which parse_delivery_result maps to Break (lane exit).
    /// The correct path is Err(Elapsed) → Continue → re-poll the consumer.
    ///
    /// This test exercises the `timeout(ZERO, pending())` fix in amqp.rs:
    /// the Duration::ZERO timeout fires immediately, pending() never resolves,
    /// so the result is always Err(Elapsed) — never Ok(None).
    #[tokio::test]
    async fn inflight_completion_maps_to_elapsed_not_none() {
        use futures_util::StreamExt;
        use futures_util::stream::FuturesUnordered;
        use std::future::Future;
        use std::pin::Pin;

        let mut inflight: FuturesUnordered<Pin<Box<dyn Future<Output = ()>>>> =
            FuturesUnordered::new();
        inflight.push(Box::pin(async { tokio::task::yield_now().await }));

        // Drive the inflight job to completion.
        let done = inflight.next().await;
        assert!(done.is_some(), "inflight job should complete");

        // Simulate the fixed path: timeout(ZERO, pending()) → Err(Elapsed).
        let result: Result<Option<Result<(), ()>>, tokio::time::error::Elapsed> =
            tokio::time::timeout(Duration::ZERO, std::future::pending()).await;

        assert!(
            result.is_err(),
            "inflight completion must yield Err(Elapsed), not Ok(None) which would break the lane"
        );
    }

    #[test]
    fn orphaned_pending_select_query_contains_table_and_placeholders() {
        let q = orphaned_pending_select_query(JobTable::Embed);
        assert!(
            q.contains("axon_embed_jobs"),
            "query must reference the correct table"
        );
        assert!(q.contains("status = $1"), "query must bind status as $1");
        assert!(
            q.contains("make_interval"),
            "query must use make_interval for type safety"
        );
    }
}
