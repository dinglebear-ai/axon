use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Build a `sqlx::Error::Database` carrying a specific SQLite result code.
fn db_err(code: &str) -> sqlx::Error {
    #[derive(Debug)]
    struct FakeDbError(String);
    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "database is locked")
        }
    }
    impl std::error::Error for FakeDbError {}
    impl sqlx::error::DatabaseError for FakeDbError {
        fn message(&self) -> &str {
            "database is locked"
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(std::borrow::Cow::Borrowed(&self.0))
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }
    sqlx::Error::Database(Box::new(FakeDbError(code.to_string())))
}

#[test]
fn busy_snapshot_is_retryable() {
    // 517 is the whole reason this module exists: `busy_timeout` cannot cover
    // it, so it MUST be classified retryable or the fix does nothing.
    assert!(is_retryable_busy(&db_err("517")));
}

#[test]
fn plain_busy_and_recovery_are_retryable() {
    assert!(is_retryable_busy(&db_err("5")));
    assert!(is_retryable_busy(&db_err("261")));
}

#[test]
fn non_busy_errors_are_not_retried() {
    // Retrying a constraint violation or a corrupt DB would mask a real fault.
    assert!(!is_retryable_busy(&db_err("19"))); // SQLITE_CONSTRAINT
    assert!(!is_retryable_busy(&db_err("11"))); // SQLITE_CORRUPT
    assert!(!is_retryable_busy(&db_err("1"))); // SQLITE_ERROR
    assert!(!is_retryable_busy(&sqlx::Error::RowNotFound));
}

#[tokio::test]
async fn succeeds_without_retry_when_op_succeeds() {
    let calls = AtomicUsize::new(0);
    let out: Result<u8, sqlx::Error> = with_busy_retry("probe", || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(7u8) }
    })
    .await;
    assert_eq!(out.expect("should succeed"), 7);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "must not retry on success");
}

#[tokio::test]
async fn retries_busy_then_succeeds() {
    let calls = AtomicUsize::new(0);
    let out: Result<&str, sqlx::Error> = with_busy_retry("probe", || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        async move { if n < 2 { Err(db_err("517")) } else { Ok("ok") } }
    })
    .await;
    assert_eq!(out.expect("should recover"), "ok");
    assert_eq!(calls.load(Ordering::SeqCst), 3, "two failures then success");
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let calls = AtomicUsize::new(0);
    let out: Result<(), sqlx::Error> = with_busy_retry("probe", || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Err(db_err("517")) }
    })
    .await;
    assert!(out.is_err(), "persistent busy must surface, not hang");
    assert_eq!(calls.load(Ordering::SeqCst), MAX_ATTEMPTS);
}

#[tokio::test]
async fn non_busy_error_fails_immediately() {
    let calls = AtomicUsize::new(0);
    let out: Result<(), sqlx::Error> = with_busy_retry("probe", || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Err(db_err("19")) }
    })
    .await;
    assert!(out.is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a constraint violation must not be retried"
    );
}
