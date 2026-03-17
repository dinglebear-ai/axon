//! Typed error enum for job lifecycle operations.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during job lifecycle operations (claim, mark, enqueue).
#[derive(Debug, Error)]
pub enum JobError {
    /// Database query or connection failure.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// AMQP publish or channel failure.
    #[error("AMQP error: {0}")]
    Amqp(#[from] lapin::Error),
    /// Job with the given ID was not found.
    #[error("job not found: {id}")]
    JobNotFound { id: Uuid },
    /// Job was already claimed by another worker.
    #[error("job already claimed")]
    AlreadyClaimed,
    /// Operation exceeded its timeout.
    #[error("operation timed out")]
    Timeout,
    /// Catch-all for unstructured errors during migration.
    #[error("{0}")]
    Other(String),
}
