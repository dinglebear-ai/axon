use std::future::Future;
use std::time::Duration;

/// Bounded retry/backoff for transient SQLite "database is locked" errors.
///
/// Retries the closure up to `MAX_ATTEMPTS` times with exponential backoff.
/// Only lock contention is retried; other errors propagate immediately.
const MAX_ATTEMPTS: u32 = 5;
const BASE_BACKOFF_MS: u64 = 25;

pub(super) async fn retry_busy<F, Fut, T>(label: &'static str, mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let mut attempt: u32 = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(err) if is_lock_busy(&err) && attempt + 1 < MAX_ATTEMPTS => {
                let backoff = BASE_BACKOFF_MS << attempt;
                tracing::debug!(
                    op = label,
                    attempt = attempt + 1,
                    backoff_ms = backoff,
                    error = %err,
                    "lite ops: SQLite busy, retrying"
                );
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

pub(super) fn is_lock_busy(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        let msg = db_err.message();
        msg.contains("database is locked") || msg.contains("database table is locked")
    } else {
        false
    }
}
