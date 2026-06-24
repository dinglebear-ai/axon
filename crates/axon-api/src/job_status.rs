use std::fmt;

use tracing;

/// Type-safe representation of the job status column values used across all
/// `axon_*_jobs` tables.
///
/// Using this enum instead of raw string literals eliminates entire classes of
/// bugs: a typo in `"completd"` compiles fine but matches zero rows in
/// SQL queries. `JobStatus::Completed.as_str()` cannot be misspelled.
///
/// # Usage in SQL
///
/// ```rust,ignore
/// # use axon_api::job_status::JobStatus;
/// # async fn example(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
/// sqlx::query("UPDATE axon_embed_jobs SET status=$1 WHERE id=$2")
///     .bind(JobStatus::Completed.as_str())
///     .bind(id)
///     .execute(pool)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Canceled,
    Unknown(String),
}

impl JobStatus {
    /// Returns the canonical string value stored in the database `status` column.
    ///
    /// All `axon_*_jobs` tables enforce a CHECK constraint that restricts the
    /// `status` column to exactly these five values. Changing a value here
    /// will break the CHECK constraint at runtime.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Parse a string into a JobStatus enum.
    ///
    /// Returns `JobStatus::Unknown` for unknown values so invalid DB or wire
    /// states do not masquerade as legitimate terminal failures.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "canceled" => Self::Canceled,
            other => {
                tracing::warn!(raw = s, "unknown job status value in DB");
                Self::Unknown(other.to_string())
            }
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "job_status_tests.rs"]
mod tests;
