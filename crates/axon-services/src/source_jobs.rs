use axon_api::source::*;
use axon_jobs::boundary::{JobStore, Result};

pub async fn create_job(
    store: &(impl JobStore + ?Sized),
    request: JobCreateRequest,
) -> Result<JobDescriptor> {
    store.create(request).await
}

pub async fn job_status(
    store: &(impl JobStore + ?Sized),
    job_id: JobId,
) -> Result<Option<SourceJobStatus>> {
    let Some(summary) = store.get(job_id).await? else {
        return Ok(None);
    };
    let attempts = store.attempts(job_id).await?;
    let stages = store.stages(job_id).await?;
    let latest_event_sequence = store.latest_event_sequence(job_id).await?;
    Ok(Some(SourceJobStatus {
        summary: summary.clone(),
        attempts,
        stages,
        latest_event_sequence,
        poll_after_ms: Some(1000),
        metadata: MetadataMap::new(),
    }))
}

pub async fn list_jobs(
    store: &(impl JobStore + ?Sized),
    request: JobListRequest,
) -> Result<Page<JobSummary>> {
    store.list(request).await
}

pub async fn list_events(
    store: &(impl JobStore + ?Sized),
    request: JobEventListRequest,
) -> Result<JobEventPage> {
    store.events(request).await
}

pub async fn cancel_job(
    store: &(impl JobStore + ?Sized),
    job_id: JobId,
    request: JobCancelRequest,
) -> Result<JobCancelResult> {
    store.cancel(job_id, request).await
}

pub async fn retry_job(
    store: &(impl JobStore + ?Sized),
    job_id: JobId,
    request: JobRetryRequest,
) -> Result<JobRetryResult> {
    store.retry(job_id, request).await
}

pub async fn recover_jobs(
    store: &(impl JobStore + ?Sized),
    request: JobRecoveryRequest,
) -> Result<JobRecoveryResult> {
    store.recover(request).await
}

pub async fn cleanup_jobs(
    store: &(impl JobStore + ?Sized),
    request: JobCleanupRequest,
) -> Result<JobCleanupResult> {
    store.cleanup(request).await
}

pub async fn list_artifacts(
    store: &(impl JobStore + ?Sized),
    request: JobArtifactListRequest,
) -> Result<JobArtifactListResult> {
    store.artifacts(request).await
}

#[cfg(test)]
#[path = "source_jobs_tests.rs"]
mod tests;
