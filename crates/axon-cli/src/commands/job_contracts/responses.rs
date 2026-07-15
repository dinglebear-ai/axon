use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct JobStatusResponse {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_json: Option<serde_json::Value>,
}

impl JobStatusResponse {
    pub fn from_service_job(job: &axon_services::types::ServiceJob) -> Self {
        super::record::SharedJobRecord::service(job).into()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobCancelResponse {
    pub id: Uuid,
    pub canceled: bool,
    pub source: &'static str,
}

impl JobCancelResponse {
    pub fn new(id: Uuid, canceled: bool) -> Self {
        Self {
            id,
            canceled,
            source: "rust",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobErrorsResponse {
    pub id: Uuid,
    pub status: String,
    pub error: Option<String>,
}

impl JobErrorsResponse {
    pub fn from_job(id: Uuid, status: String, error: Option<String>) -> Self {
        Self { id, status, error }
    }
}
