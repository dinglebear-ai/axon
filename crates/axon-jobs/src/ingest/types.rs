use crate::status::JobStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// The transport-neutral ingest-source DTOs moved to `axon_api::ingest`;
// re-exported here so existing `crate::ingest::*` call sites resolve.
pub use axon_api::ingest::{
    IngestJobConfig, IngestSource, RE_INGESTABLE_SOURCE_TYPES, source_type_label, target_label,
};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct IngestJob {
    pub id: Uuid,
    /// Raw status string from the database. Use [`IngestJob::status()`] for
    /// type-safe access when `JobStatus` gains `sqlx::Type` derive.
    pub status: String,
    pub source_type: String,
    pub target: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: serde_json::Value,
}

impl IngestJob {
    /// Parse the raw `status` string into a typed [`JobStatus`].
    ///
    /// Returns `None` if the string doesn't match any known variant (shouldn't
    /// happen with the CHECK constraint, but defensive is correct).
    pub fn status(&self) -> Option<JobStatus> {
        match self.status.as_str() {
            "pending" => Some(JobStatus::Pending),
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "canceled" => Some(JobStatus::Canceled),
            _ => None,
        }
    }
}
