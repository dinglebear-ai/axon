use crate::crates::jobs::status::JobStatus;
use crate::crates::services::types::ServiceJob;

pub(super) fn graph_to_service_job(job: crate::crates::jobs::graph::GraphJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: Some(job.url),
        source_type: None,
        target: None,
        urls_json: None,
        result_json: Some(serde_json::json!({
            "chunk_count": job.chunk_count,
            "entity_count": job.entity_count,
            "relation_count": job.relation_count,
        })),
        config_json: None,
    }
}

pub(super) fn crawl_to_service_job(job: crate::crates::jobs::crawl::CrawlJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: Some(job.url),
        source_type: None,
        target: None,
        urls_json: None,
        result_json: job.result_json,
        config_json: None,
    }
}

pub(super) fn embed_to_service_job(job: crate::crates::jobs::embed::EmbedJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: Some(job.input_text),
        urls_json: None,
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

pub(super) fn extract_to_service_job(job: crate::crates::jobs::extract::ExtractJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: None,
        urls_json: Some(job.urls_json),
        result_json: job.result_json,
        config_json: None,
    }
}

pub(super) fn ingest_to_service_job(job: crate::crates::jobs::ingest::IngestJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: Some(job.source_type),
        target: Some(job.target),
        urls_json: None,
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

pub(super) fn refresh_to_service_job(job: crate::crates::jobs::refresh::RefreshJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: None,
        urls_json: Some(job.urls_json),
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

#[allow(dead_code)]
pub(super) fn has_active_status(status: JobStatus) -> bool {
    matches!(status, JobStatus::Pending | JobStatus::Running)
}
