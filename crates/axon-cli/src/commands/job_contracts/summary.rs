use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct JobSummaryEntry {
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

impl JobSummaryEntry {
    pub fn from_crawl(job: &axon_jobs::crawl::CrawlJob) -> Self {
        super::record::SharedJobRecord::crawl(job).into()
    }

    pub fn from_extract(job: &axon_jobs::extract::ExtractJob) -> Self {
        super::record::SharedJobRecord::extract(job).into()
    }

    pub fn from_ingest(job: &axon_jobs::ingest::IngestJob) -> Self {
        super::record::SharedJobRecord::ingest(job).into()
    }

    pub fn from_embed(job: &axon_jobs::embed::EmbedJob) -> Self {
        super::record::SharedJobRecord::embed(job).into()
    }

    pub fn from_service_job(job: &axon_services::types::ServiceJob) -> Self {
        super::record::SharedJobRecord::service(job).into()
    }
}
