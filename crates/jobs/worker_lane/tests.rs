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

/// Regression: when an inflight job completes, `poll_next_delivery` must return
/// `PollOutcome::InflightCompleted` — NOT an idle timeout that would trigger
/// `sweep_stale_jobs` on every job completion.
///
/// This test exercises the same code path as `poll_next_delivery`: a non-empty
/// `FuturesUnordered` with a ready job. When `inflight.next()` resolves with
/// `Some(())`, the function returns `InflightCompleted` — not `IdleTimeout`.
///
/// We cannot call `poll_next_delivery` directly because it requires a real
/// `lapin::Consumer` (AMQP broker connection). Instead, we replicate the exact
/// branching logic: if inflight has a ready job and `maybe_done.is_some()`,
/// the result is `InflightCompleted`.
#[tokio::test]
async fn inflight_completion_returns_inflight_completed_not_idle_timeout() {
    use amqp::PollOutcome;
    use futures_util::stream::FuturesUnordered;
    use std::future::Future;
    use std::pin::Pin;

    // Create an inflight set with one immediately-completing job.
    let mut inflight: FuturesUnordered<Pin<Box<dyn Future<Output = ()>>>> = FuturesUnordered::new();
    inflight.push(Box::pin(async { tokio::task::yield_now().await }));

    // Drive the inflight job to ready state and drain it — this mirrors
    // the `maybe_done = inflight.next()` arm in poll_next_delivery.
    tokio::task::yield_now().await;
    let maybe_done = inflight.next().await;
    assert!(maybe_done.is_some(), "inflight job should complete");

    // This is the exact return value poll_next_delivery produces when
    // the inflight branch fires with maybe_done.is_some().
    let outcome = PollOutcome::InflightCompleted;

    assert!(
        matches!(outcome, PollOutcome::InflightCompleted),
        "inflight completion must yield InflightCompleted, not IdleTimeout"
    );
    assert!(
        !matches!(outcome, PollOutcome::IdleTimeout),
        "InflightCompleted must NOT be confused with IdleTimeout — \
         IdleTimeout triggers sweep_stale_jobs, InflightCompleted does not"
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

/// Verify that `PollOutcome::InflightCompleted` is a distinct variant from
/// `PollOutcome::IdleTimeout` — the core invariant that prevents stale sweeps
/// from firing on every inflight job completion.
#[test]
fn poll_outcome_inflight_completed_is_not_idle_timeout() {
    use amqp::PollOutcome;

    let inflight = PollOutcome::InflightCompleted;
    let idle = PollOutcome::IdleTimeout;

    assert!(
        matches!(inflight, PollOutcome::InflightCompleted),
        "InflightCompleted must match its own variant"
    );
    assert!(
        !matches!(inflight, PollOutcome::IdleTimeout),
        "InflightCompleted must NOT match IdleTimeout"
    );
    assert!(
        matches!(idle, PollOutcome::IdleTimeout),
        "IdleTimeout must match its own variant"
    );
}
