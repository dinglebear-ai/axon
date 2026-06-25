//! `From<*Job>` conversions into the transport-neutral `axon_api::ServiceJob`.
//!
//! These live here (not in `axon-api`) because the source job types are local
//! to `axon-jobs`; the orphan rule permits `impl From<LocalJob> for ServiceJob`
//! here, and this keeps `axon-api` free of any dependency on the job structs.
//!
//! NOTE — these conversions are the **lossy/legacy** construction path: the
//! per-`*Job` structs carry no attempt/reclaim metadata, so the resulting
//! `ServiceJob` zeroes `attempt_count` and leaves `active_attempt_id` /
//! `last_reclaimed_at` / `last_reclaimed_reason` unset. The authoritative,
//! full-fidelity path that hydrates those fields from the DB row is
//! `service_job_from_row` in `query.rs`; the service runtime uses that for
//! `job_status`/`list_jobs`. Prefer it whenever reclaim/attempt data matters.

use axon_api::service_job::ServiceJob;

impl From<crate::crawl::CrawlJob> for ServiceJob {
    fn from(job: crate::crawl::CrawlJob) -> Self {
        Self {
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
            progress_json: None,
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::embed::EmbedJob> for ServiceJob {
    fn from(job: crate::embed::EmbedJob) -> Self {
        Self {
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
            progress_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::extract::ExtractJob> for ServiceJob {
    fn from(job: crate::extract::ExtractJob) -> Self {
        Self {
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
            progress_json: None,
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::ingest::IngestJob> for ServiceJob {
    fn from(job: crate::ingest::IngestJob) -> Self {
        Self {
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
            progress_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}
