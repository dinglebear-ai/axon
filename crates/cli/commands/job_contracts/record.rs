use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::crates::jobs::crawl::CrawlJob;
use crate::crates::jobs::embed::EmbedJob;
use crate::crates::jobs::extract::ExtractJob;
use crate::crates::jobs::ingest::IngestJob;
use crate::crates::services::types::ServiceJob;

use super::responses::JobStatusResponse;
use super::summary::JobSummaryEntry;

pub(super) struct SharedJobRecord {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub urls: Option<serde_json::Value>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: Option<serde_json::Value>,
}

pub(super) fn payload_string(payload: Option<&serde_json::Value>, key: &str) -> Option<String> {
    payload
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

impl SharedJobRecord {
    pub(super) fn crawl(job: &CrawlJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: Some(job.url.clone()),
            source_type: None,
            target: None,
            urls: None,
            result_json: job.result_json.clone(),
            config_json: None,
        }
    }

    pub(super) fn extract(job: &ExtractJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: None,
            source_type: None,
            target: None,
            urls: Some(job.urls_json.clone()),
            result_json: job.result_json.clone(),
            config_json: None,
        }
    }

    pub(super) fn ingest(job: &IngestJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: None,
            source_type: Some(job.source_type.clone()),
            target: Some(job.target.clone()),
            urls: None,
            result_json: job.result_json.clone(),
            config_json: Some(job.config_json.clone()),
        }
    }

    pub(super) fn embed(job: &EmbedJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: None,
            source_type: None,
            target: Some(job.input_text.clone()),
            urls: None,
            result_json: job.result_json.clone(),
            config_json: Some(job.config_json.clone()),
        }
    }

    pub(super) fn service(job: &ServiceJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: job.url.clone(),
            source_type: job.source_type.clone(),
            target: job.target.clone(),
            urls: job.urls_json.clone(),
            result_json: job.result_json.clone(),
            config_json: job.config_json.clone(),
        }
    }
}

impl From<SharedJobRecord> for JobStatusResponse {
    fn from(value: SharedJobRecord) -> Self {
        let collection = payload_string(value.result_json.as_ref(), "collection")
            .or_else(|| payload_string(value.config_json.as_ref(), "collection"));
        let source = payload_string(value.result_json.as_ref(), "source");
        Self {
            id: value.id,
            status: value.status,
            created_at: value.created_at,
            updated_at: value.updated_at,
            started_at: value.started_at,
            finished_at: value.finished_at,
            error: value.error_text.clone(),
            error_text: value.error_text,
            url: value.url,
            source_type: value.source_type,
            target: value.target,
            urls: value.urls.clone(),
            urls_json: value.urls,
            metrics: value.result_json.clone(),
            collection,
            source,
            result_json: value.result_json,
            config_json: value.config_json,
        }
    }
}

impl From<SharedJobRecord> for JobSummaryEntry {
    fn from(value: SharedJobRecord) -> Self {
        let r: JobStatusResponse = value.into();
        Self {
            id: r.id,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
            error: r.error,
            error_text: r.error_text,
            url: r.url,
            source_type: r.source_type,
            target: r.target,
            urls: r.urls,
            urls_json: r.urls_json,
            metrics: r.metrics,
            collection: r.collection,
            source: r.source,
            result_json: r.result_json,
            config_json: r.config_json,
        }
    }
}
