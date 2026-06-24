use crate::context::ServiceContext;
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobListResult,
    JobStartOutcome, StartDisposition,
};
use axon_core::config::Config;
use axon_jobs::backend::{JobKind, JobPayload};
use axon_jobs::config_snapshot::ingest_config_json;
use axon_jobs::ingest::types::{source_type_label, target_label};
pub use axon_jobs::ingest::{IngestJob, IngestSource};
use axon_jobs::ingest::{count_ingest_jobs, get_ingest_job, list_ingest_jobs, start_ingest_job};
use std::error::Error;
use uuid::Uuid;

pub mod classify;
mod prepared_sessions;
pub mod request;
pub use axon_ingest::orchestrate::{
    ingest_generic_git_with_progress, ingest_gitea_with_progress, ingest_github,
    ingest_github_with_progress, ingest_gitlab_with_progress, ingest_payload, ingest_reddit,
    ingest_reddit_with_progress, ingest_reddit_with_progress_and_options, ingest_rss,
    ingest_rss_with_progress, ingest_sessions, ingest_sessions_prepared_with_progress,
    ingest_sessions_with_progress, ingest_youtube, ingest_youtube_with_progress, map_ingest_result,
};
pub use classify::classify_target;
pub use prepared_sessions::ingest_sessions_prepared_start_with_context;
pub use request::{source_from_mcp_request, validate_ingest_source};

pub fn map_ingest_start_result(job_id: String) -> IngestStartResult {
    IngestStartResult { job_id }
}

pub fn map_ingest_job_result(payload: serde_json::Value) -> IngestJobResult {
    IngestJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn ingest_start(
    cfg: &Config,
    source: IngestSource,
) -> Result<IngestStartResult, Box<dyn Error>> {
    let job_id = start_ingest_job(cfg, source).await?;
    Ok(map_ingest_start_result(job_id.to_string()))
}

pub async fn ingest_start_with_context(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    // Always route through service_context.jobs.enqueue() so that notify()
    // fires immediately and workers wake without 0-5 second polling delay.
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    let config_json = ingest_config_json(cfg, &source)?;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Ingest {
            target,
            source_type,
            config_json,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(job_id.to_string()),
    })
}

pub async fn ingest_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Ingest, id).await?;
    Ok(job.map(|value| {
        map_ingest_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn ingest_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<IngestResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Ingest, limit, offset).await?;
    Ok(map_ingest_result(serde_json::to_value(jobs)?))
}

pub async fn ingest_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Ingest, id).await
}

pub async fn ingest_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_count(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    count_ingest_jobs(cfg).await
}

pub async fn ingest_status_raw(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<IngestJob>, Box<dyn Error>> {
    get_ingest_job(cfg, id).await
}

pub async fn ingest_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<IngestJob>, Box<dyn Error>> {
    let (jobs, total) = tokio::join!(
        list_ingest_jobs(cfg, None, limit, offset),
        count_ingest_jobs(cfg),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}

pub async fn ingest_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Ingest).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
