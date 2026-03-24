use std::fmt;

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
/// # use axon::crates::jobs::status::JobStatus;
/// # async fn example(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), sqlx::Error> {
/// sqlx::query("UPDATE axon_embed_jobs SET status=$1 WHERE id=$2")
///     .bind(JobStatus::Completed.as_str())
///     .bind(id)
///     .execute(pool)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Canceled,
}

impl JobStatus {
    /// Returns the canonical string value stored in the database `status` column.
    ///
    /// All `axon_*_jobs` tables enforce a CHECK constraint that restricts the
    /// `status` column to exactly these five values. Changing a value here
    /// will break the CHECK constraint at runtime.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    /// Parse a string into a JobStatus enum.
    ///
    /// Returns `JobStatus::Failed` for unknown values. Callers should validate
    /// the input string against the database CHECK constraint separately if needed.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "canceled" => Self::Canceled,
            _ => Self::Failed,
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_returns_expected_values() {
        assert_eq!(JobStatus::Pending.as_str(), "pending");
        assert_eq!(JobStatus::Running.as_str(), "running");
        assert_eq!(JobStatus::Completed.as_str(), "completed");
        assert_eq!(JobStatus::Failed.as_str(), "failed");
        assert_eq!(JobStatus::Canceled.as_str(), "canceled");
    }

    #[test]
    fn display_matches_as_str() {
        for status in [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Canceled,
        ] {
            assert_eq!(format!("{status}"), status.as_str());
        }
    }

    #[test]
    fn all_variants_have_unique_string_representations() {
        let variants = [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Canceled,
        ];
        let strings: std::collections::HashSet<_> = variants.iter().map(|s| s.as_str()).collect();
        assert_eq!(strings.len(), variants.len());
    }
}
