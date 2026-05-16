use std::error::Error;
use std::fmt;

use uuid::Uuid;

use crate::jobs::backend::JobKind;
use crate::services::context::ServiceContext;
pub use crate::services::runtime::WorkerMode;
use crate::services::types::ServiceJob;

// Helper: downgrade Send+Sync error to plain Box<dyn Error> for callers that don't need Send+Sync.
// Wraps the original error to preserve the Display output and source chain without stringifying.
fn downgrade(e: Box<dyn Error + Send + Sync>) -> Box<dyn Error> {
    struct Wrapper(Box<dyn Error + Send + Sync>);
    impl fmt::Display for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
    impl fmt::Debug for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
    impl Error for Wrapper {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(self.0.as_ref())
        }
    }
    Box::new(Wrapper(e))
}

pub async fn list_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .list_jobs(kind, limit, offset)
        .await
        .map_err(downgrade)
}

pub async fn list_ingest_jobs(
    service_context: &ServiceContext,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .list_ingest_jobs(source_filter, limit, offset)
        .await
        .map_err(downgrade)
}

pub async fn job_status(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .job_status(kind, id)
        .await
        .map_err(downgrade)
}

pub async fn cancel_job(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    service_context
        .jobs
        .cancel_job(kind, id)
        .await
        .map_err(downgrade)
}

pub async fn cleanup_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context
        .jobs
        .cleanup_jobs(kind)
        .await
        .map_err(downgrade)
}

pub async fn clear_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context
        .jobs
        .clear_jobs(kind)
        .await
        .map_err(downgrade)
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
        .max(0)
        * 1_000;
    service_context
        .jobs
        .recover_jobs(kind, stale_threshold_ms)
        .await
        .map_err(downgrade)
}

pub async fn start_worker(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<WorkerMode, Box<dyn Error>> {
    service_context
        .jobs
        .start_worker(kind)
        .await
        .map_err(downgrade)
}

pub async fn notify_worker(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<(), Box<dyn Error>> {
    service_context
        .jobs
        .notify_worker(kind)
        .await
        .map_err(downgrade)
}

pub async fn drain_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<WorkerMode, Box<dyn Error>> {
    service_context
        .jobs
        .drain_jobs(kind)
        .await
        .map_err(downgrade)
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
