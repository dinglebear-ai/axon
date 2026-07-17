//! `ServiceJob` — the rich, transport-neutral job view returned by the service
//! job runtime to CLI/MCP/HTTP callers.
//!
//! Lives here in `axon-api` (not `services`) so `axon-jobs` can construct it
//! directly (breaking the historical `jobs` ↔ `services` cycle). The
//! `From<jobs::*Job>` conversions live in `axon-jobs`, where the source job
//! types are local.

use crate::job_status::JobStatus;
use crate::source::JobKind;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceJob {
    pub id: uuid::Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub urls_json: Option<serde_json::Value>,
    pub progress_json: Option<serde_json::Value>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: Option<serde_json::Value>,
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reclaimed_reason: Option<String>,
}

impl ServiceJob {
    pub fn status_enum(&self) -> JobStatus {
        JobStatus::from_str(&self.status)
    }
}

/// Canonical status projection for one durable job.
///
/// This intentionally does not expose the storage-oriented `*_json` fields or
/// synthesize family-specific metrics. Status transports serialize this DTO
/// directly and preserve progress/result as separate values.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusJob {
    pub job_id: uuid::Uuid,
    pub kind: JobKind,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    pub attempt: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reclaimed_reason: Option<String>,
}

impl StatusJob {
    pub fn from_service_job(kind: JobKind, job: &ServiceJob) -> Self {
        Self {
            job_id: job.id,
            kind,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error: job.error_text.clone(),
            source: job.target.clone().or_else(|| job.url.clone()),
            progress: job.progress_json.clone(),
            result: job.result_json.clone(),
            attempt: job.attempt_count,
            active_attempt_id: job.active_attempt_id.clone(),
            last_reclaimed_at: job.last_reclaimed_at,
            last_reclaimed_reason: job.last_reclaimed_reason.clone(),
        }
    }
}

#[cfg(test)]
#[path = "service_job_tests.rs"]
mod tests;
