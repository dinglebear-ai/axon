//! Thin wrappers around the unified `JobStore` for list/status/events/
//! artifacts/cancel/retry/recover/cleanup/clear. Split out of `jobs.rs` to
//! keep it under the monolith line cap; all of these share the
//! `call_job_store` dispatch helper and `box_send_sync` error wrapper.

use std::error::Error;
use std::future::Future;

use crate::context::ServiceContext;
use axon_api::source::{
    JobCancelRequest, JobCancelResult, JobCleanupRequest, JobCleanupResult, JobClearRequest,
    JobClearResult, JobEventListRequest, JobEventPage, JobListRequest, JobRecoveryRequest,
    JobRecoveryResult, JobRetryRequest, JobRetryResult, Page, Severity, SourceWarning,
};

pub async fn list_unified_jobs(
    service_context: &ServiceContext,
    request: JobListRequest,
) -> Result<Page<axon_api::source::JobSummary>, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        let mut page = store.list(request).await?;
        for summary in &mut page.items {
            hydrate_terminal_counts(store.as_ref(), summary).await?;
        }
        Ok(page)
    })
    .await
}

pub async fn unified_job_status(
    service_context: &ServiceContext,
    job_id: axon_api::source::JobId,
) -> Result<Option<axon_api::source::JobSummary>, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        let Some(mut summary) = store.get(job_id).await? else {
            return Ok(None);
        };
        hydrate_terminal_counts(store.as_ref(), &mut summary).await?;
        Ok(Some(summary))
    })
    .await
}

async fn hydrate_terminal_counts(
    store: &dyn axon_jobs::boundary::JobStore,
    summary: &mut axon_api::source::JobSummary,
) -> Result<(), axon_api::source::ApiError> {
    let counts_empty = summary.counts.as_ref().is_none_or(|counts| {
        counts.items_done == 0
            && counts.documents_done == 0
            && counts.chunks_done == 0
            && counts.bytes_done == 0
    });
    if counts_empty
        && matches!(
            summary.status,
            axon_api::source::LifecycleStatus::Completed
                | axon_api::source::LifecycleStatus::CompletedDegraded
                | axon_api::source::LifecycleStatus::Failed
                | axon_api::source::LifecycleStatus::Expired
                | axon_api::source::LifecycleStatus::Canceled
                | axon_api::source::LifecycleStatus::Skipped
        )
    {
        let recovered = store.terminal_counts(summary.job_id).await?;
        if recovered.as_ref().is_some_and(|counts| {
            counts.items_done > 0
                || counts.documents_done > 0
                || counts.chunks_done > 0
                || counts.bytes_done > 0
        }) {
            summary.counts = recovered;
        }
    }
    Ok(())
}

pub async fn unified_job_events(
    service_context: &ServiceContext,
    request: JobEventListRequest,
) -> Result<JobEventPage, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.events(request).await
    })
    .await
}

pub async fn unified_job_artifacts(
    service_context: &ServiceContext,
    request: axon_api::source::JobArtifactListRequest,
) -> Result<axon_api::source::JobArtifactListResult, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.artifacts(request).await
    })
    .await
}

pub async fn cancel_unified_job(
    service_context: &ServiceContext,
    job_id: axon_api::source::JobId,
    request: JobCancelRequest,
) -> Result<JobCancelResult, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.cancel(job_id, request).await
    })
    .await
}

pub async fn retry_unified_job(
    service_context: &ServiceContext,
    job_id: axon_api::source::JobId,
    request: JobRetryRequest,
) -> Result<JobRetryResult, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.retry(job_id, request).await
    })
    .await
}

pub async fn recover_unified_jobs(
    service_context: &ServiceContext,
    request: JobRecoveryRequest,
) -> Result<JobRecoveryResult, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.recover(request).await
    })
    .await
}

pub async fn cleanup_unified_jobs(
    service_context: &ServiceContext,
    request: JobCleanupRequest,
) -> Result<JobCleanupResult, Box<dyn Error + Send + Sync>> {
    call_job_store(service_context, |store| async move {
        store.cleanup(request).await
    })
    .await
}

pub async fn clear_unified_jobs(
    service_context: &ServiceContext,
    request: JobClearRequest,
) -> Result<JobClearResult, Box<dyn Error + Send + Sync>> {
    if !request.confirm {
        return Err(box_send_sync(
            "job clear requires confirm=true and admin authorization",
        ));
    }
    let store = service_context
        .job_store()
        .ok_or_else(|| box_send_sync("unified job store is not available"))?;
    let mut deleted = 0_u64;
    loop {
        let result = store
            .cleanup(JobCleanupRequest {
                dry_run: false,
                kind: request.kind,
                older_than: request.older_than.clone(),
                status: request.status,
                limit: Some(500),
                older_than_seconds: None,
                confirm_all_terminal: true,
            })
            .await
            .map_err(|error| box_send_sync(error.message))?;
        deleted += result.deleted;
        if result.deleted == 0 || result.deleted < 500 {
            break;
        }
    }
    Ok(JobClearResult {
        deleted,
        status: request.status,
        warnings: vec![SourceWarning {
            code: "jobs.clear_terminal_only".to_string(),
            severity: Severity::Info,
            message: "clear pruned terminal jobs only; active jobs require cancel/recover first"
                .to_string(),
            source_item_key: None,
            retryable: false,
        }],
    })
}

pub(crate) fn box_send_sync(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

async fn call_job_store<T, F, Fut>(
    service_context: &ServiceContext,
    f: F,
) -> Result<T, Box<dyn Error + Send + Sync>>
where
    F: FnOnce(std::sync::Arc<dyn axon_jobs::boundary::JobStore>) -> Fut,
    Fut: Future<Output = axon_jobs::boundary::Result<T>>,
{
    let store = service_context
        .job_store()
        .ok_or_else(|| box_send_sync("unified job store is not available"))?;
    f(store).await.map_err(|error| box_send_sync(error.message))
}
