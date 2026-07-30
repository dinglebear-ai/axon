//! Retry wrapper for SQLite busy conditions that `busy_timeout` cannot cover.
//!
//! # Why this exists
//!
//! The shared pool already sets `busy_timeout=30000`, and that genuinely works
//! for plain `SQLITE_BUSY` (5): SQLite parks inside the C library and retries
//! the lock acquisition itself.
//!
//! It does **not** cover `SQLITE_BUSY_SNAPSHOT` (517). That is not lock
//! contention — it means this connection is holding a read transaction whose
//! snapshot has been overtaken by another writer's commit. SQLite's busy
//! handler deliberately declines to retry it, because parking would never
//! resolve the condition: the stale snapshot has to be abandoned and the
//! statement re-run. Raising `busy_timeout` cannot fix a 517, at any value.
//!
//! Measured 2026-07-29: with several `axon` processes sharing `~/.axon/jobs.db`
//! (176 MB), 2 of 8 concurrent `map` runs failed against *distinct* URLs — so
//! with no source-lease overlap at all — and the first error in each failing
//! chain was `(code: 517) database is locked`. The same workload run serially
//! completes in 1.76 s. After the container was repointed at the live data
//! tree, `axon serve` began logging the same 517 against worker heartbeats.

use std::time::Duration;

/// Attempts, including the first. Four attempts spans ~700 ms of backoff, which
/// comfortably outlives a WAL checkpoint without stalling an interactive call.
const MAX_ATTEMPTS: usize = 4;

/// Base backoff; doubles each attempt (50ms, 100ms, 200ms, 400ms).
const BASE_BACKOFF: Duration = Duration::from_millis(50);

/// True when the error is a SQLite busy condition worth re-running.
///
/// Matches both the plain busy code and the extended snapshot code. The plain
/// code is included because `busy_timeout` can still expire under sustained
/// write pressure, and a retry is cheaper than failing the job.
pub fn is_retryable_busy(err: &sqlx::Error) -> bool {
    let sqlx::Error::Database(db) = err else {
        return false;
    };
    // SQLite surfaces 5 = SQLITE_BUSY, 517 = SQLITE_BUSY_SNAPSHOT,
    // 261 = SQLITE_BUSY_RECOVERY. All three are transient by definition.
    matches!(db.code().as_deref(), Some("5") | Some("517") | Some("261"))
}

/// True when a rendered error string names a transient SQLite busy condition.
///
/// For layers that have already wrapped `sqlx::Error` into a domain error and
/// no longer expose the code — the store crates render it as
/// `error returned from database: (code: 517) database is locked`.
pub fn message_is_retryable_busy(rendered: &str) -> bool {
    rendered.contains("(code: 517)")
        || rendered.contains("(code: 261)")
        || (rendered.contains("(code: 5)") && rendered.contains("database is locked"))
}

/// Generic retry over any error type, given a classifier.
///
/// `op` must be safe to run more than once — use it for whole transactions or
/// idempotent statements, never a partially-applied multi-statement sequence.
pub async fn retry_on<T, E, F, Fut>(
    what: &str,
    is_busy: impl Fn(&E) -> bool,
    mut op: F,
) -> Result<T, E>
where
    E: std::fmt::Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut backoff = BASE_BACKOFF;
    for attempt in 1..=MAX_ATTEMPTS {
        match op().await {
            Ok(value) => {
                if attempt > 1 {
                    crate::logging::log_info(&format!(
                        "sqlite: {what} succeeded on attempt {attempt} after a busy retry"
                    ));
                }
                return Ok(value);
            }
            Err(err) if is_busy(&err) && attempt < MAX_ATTEMPTS => {
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
            Err(err) => {
                if is_busy(&err) {
                    crate::logging::log_warn(&format!(
                        "sqlite: {what} still busy after {MAX_ATTEMPTS} attempts: {err}"
                    ));
                }
                return Err(err);
            }
        }
    }
    unreachable!("loop returns on the final attempt")
}

/// Run `op`, re-running it on a transient SQLite busy condition.
///
/// `op` is a closure returning a fresh future each attempt — it must be safe to
/// execute more than once. Only use this for operations that are idempotent or
/// wrapped in a transaction that rolls back on failure; a partially-applied
/// multi-statement sequence must not be retried piecewise.
pub async fn with_busy_retry<T, F, Fut>(what: &str, op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    retry_on(what, is_retryable_busy, op).await
}

#[cfg(test)]
#[path = "busy_retry_tests.rs"]
mod tests;
