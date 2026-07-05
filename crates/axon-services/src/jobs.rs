use std::error::Error;
use std::fmt;
use std::future::Future;

use uuid::Uuid;

use crate::context::ServiceContext;
pub use crate::runtime::WorkerMode;
use crate::types::ServiceJob;
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCancelRequest, JobCancelResult, JobCleanupRequest,
    JobCleanupResult, JobClearRequest, JobClearResult, JobCreateRequest, JobDescriptor,
    JobEventListRequest, JobEventPage, JobExecutionMode, JobIntent, JobListRequest, JobPolicy,
    JobPriority, JobRecoveryRequest, JobRecoveryResult, JobRetryRequest, JobRetryResult,
    JobStagePlan, MetadataMap, OperationKind, Page, PipelinePhase, SourceWarning,
    job_policy_for_operation,
};
use axon_jobs::backend::JobKind;

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

pub async fn enqueue_operation(
    service_context: &ServiceContext,
    operation: OperationKind,
    mode: JobExecutionMode,
    request: serde_json::Value,
) -> Result<Option<JobDescriptor>, Box<dyn Error>> {
    if job_policy_for_operation(operation, mode) == JobPolicy::Synchronous {
        return Ok(None);
    }
    let store = service_context.job_store().ok_or_else(|| {
        Box::<dyn Error>::from("unified job store is not available for this runtime")
    })?;
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: job_kind_for_operation(operation),
            job_intent: job_intent_for_operation(operation),
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: request
                .get("idempotency_key")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            stage_plan: stage_plan_for_operation(operation),
            request: Some(request),
            auth_snapshot: AuthSnapshot::trusted_system("runtime"),
            config_snapshot_id: Some(ConfigSnapshotId::new("runtime")),
            requirements: MetadataMap::new(),
            result_schema: Some(result_schema_for_operation(operation).to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|error| Box::<dyn Error>::from(error.message))?;
    Ok(Some(descriptor))
}

pub async fn list_unified_jobs(
    service_context: &ServiceContext,
    request: JobListRequest,
) -> Result<Page<axon_api::source::JobSummary>, Box<dyn Error + Send + Sync>> {
    call_job_store(
        service_context,
        |store| async move { store.list(request).await },
    )
    .await
}

pub async fn unified_job_status(
    service_context: &ServiceContext,
    job_id: axon_api::source::JobId,
) -> Result<Option<axon_api::source::JobSummary>, Box<dyn Error + Send + Sync>> {
    call_job_store(
        service_context,
        |store| async move { store.get(job_id).await },
    )
    .await
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
            severity: axon_api::source::Severity::Info,
            message: "clear pruned terminal jobs only; active jobs require cancel/recover first"
                .to_string(),
            source_item_key: None,
            retryable: false,
        }],
    })
}

fn box_send_sync(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
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

fn job_kind_for_operation(operation: OperationKind) -> axon_api::source::JobKind {
    match operation {
        OperationKind::Source => axon_api::source::JobKind::Source,
        OperationKind::Watch => axon_api::source::JobKind::Watch,
        OperationKind::Extract => axon_api::source::JobKind::Extract,
        OperationKind::Research => axon_api::source::JobKind::Research,
        OperationKind::MemoryCompaction | OperationKind::MemoryImport => {
            axon_api::source::JobKind::Memory
        }
        OperationKind::GraphMutation => axon_api::source::JobKind::Graph,
        OperationKind::Prune => axon_api::source::JobKind::Prune,
        OperationKind::ProviderProbe => axon_api::source::JobKind::ProviderProbe,
        OperationKind::Reset => axon_api::source::JobKind::Reset,
        OperationKind::Query => axon_api::source::JobKind::Query,
        OperationKind::Retrieve => axon_api::source::JobKind::Retrieve,
    }
}

fn job_intent_for_operation(operation: OperationKind) -> JobIntent {
    match operation {
        OperationKind::Watch => JobIntent::Watch,
        OperationKind::ProviderProbe => JobIntent::Probe,
        OperationKind::Reset => JobIntent::Reset,
        OperationKind::Prune => JobIntent::Cleanup,
        _ => JobIntent::Run,
    }
}

fn stage_plan_for_operation(operation: OperationKind) -> Vec<JobStagePlan> {
    let phase = match operation {
        OperationKind::Source => PipelinePhase::Fetching,
        OperationKind::Watch => PipelinePhase::Diffing,
        OperationKind::Extract => PipelinePhase::Parsing,
        OperationKind::Research => PipelinePhase::Synthesizing,
        OperationKind::MemoryCompaction | OperationKind::MemoryImport => PipelinePhase::Preparing,
        OperationKind::GraphMutation => PipelinePhase::Graphing,
        OperationKind::Prune => PipelinePhase::Cleaning,
        OperationKind::ProviderProbe => PipelinePhase::Evaluating,
        OperationKind::Reset => PipelinePhase::Cleaning,
        OperationKind::Query | OperationKind::Retrieve => PipelinePhase::Retrieving,
    };
    vec![JobStagePlan {
        phase,
        required: true,
        provider_requirements: Vec::new(),
        estimated_items: None,
    }]
}

fn result_schema_for_operation(operation: OperationKind) -> &'static str {
    match operation {
        OperationKind::Source => "source_result",
        OperationKind::Watch => "watch_result",
        OperationKind::Extract => "extract_result",
        OperationKind::Research => "research_result",
        OperationKind::MemoryCompaction | OperationKind::MemoryImport => "memory_result",
        OperationKind::GraphMutation => "graph_result",
        OperationKind::Prune => "prune_result",
        OperationKind::ProviderProbe => "provider_probe_result",
        OperationKind::Reset => "reset_result",
        OperationKind::Query => "query_result",
        OperationKind::Retrieve => "retrieve_result",
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
