use std::error::Error;

use uuid::Uuid;

use crate::crates::jobs::backend::JobKind;
use crate::crates::services::context::ServiceContext;
pub use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::ServiceJob;

pub async fn list_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    service_context.jobs.list_jobs(kind, limit, offset).await
}

pub async fn job_status(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    service_context.jobs.job_status(kind, id).await
}

pub async fn cancel_job(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    service_context.jobs.cancel_job(kind, id).await
}

pub async fn cleanup_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context.jobs.cleanup_jobs(kind).await
}

pub async fn clear_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context.jobs.clear_jobs(kind).await
}

pub async fn job_errors(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<String>, Box<dyn Error>> {
    Ok(job_status(service_context, kind, id)
        .await?
        .and_then(|job| job.error_text))
}

pub async fn recover_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    let stale_threshold_ms = (service_context.cfg.watchdog_stale_timeout_secs
        + service_context.cfg.watchdog_confirm_secs)
        * 1_000;
    service_context
        .jobs
        .recover_jobs(kind, stale_threshold_ms)
        .await
}

pub async fn run_worker(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<WorkerMode, Box<dyn Error>> {
    service_context.jobs.run_worker(kind).await
}
