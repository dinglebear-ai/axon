//! Typed error enum for job lifecycle operations.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during job lifecycle operations (claim, mark, enqueue).
#[derive(Debug, Error)]
pub enum JobError {
    /// Database query or connection failure.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// Job with the given ID was not found.
    #[error("job not found: {id}")]
    JobNotFound { id: Uuid },
    /// Job was already claimed by another worker.
    #[error("job already claimed")]
    AlreadyClaimed,
    /// Operation exceeded its timeout.
    #[error("operation timed out")]
    Timeout,
    /// New job submission rejected because the per-queue pending cap was reached.
    ///
    /// `kind` is the human-readable queue name (e.g. `"crawl"`, `"embed"`).
    /// `current` is the count of pending jobs at the time of the check.
    /// `cap` is the configured cap (always > 0; cap=0 means unlimited and never rejects).
    #[error(
        "{kind} queue is at capacity ({current} pending jobs, max {cap}); \
         wait for workers to drain or raise the queue cap env var"
    )]
    QueueCapacityExceeded {
        kind: &'static str,
        cap: u64,
        current: u64,
    },
    /// Catch-all for unstructured errors during migration.
    #[error("{0}")]
    Other(String),
}
