use std::error::Error;
use std::fmt;

use uuid::Uuid;

use crate::context::ServiceContext;
pub use crate::runtime::WorkerMode;
use crate::types::ServiceJob;
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobDescriptor, JobExecutionMode, JobIntent,
    JobPolicy, JobPriority, JobStagePlan, JobStatusUpdate, LifecycleStatus, MetadataMap,
    OperationKind, PipelinePhase, Severity, SourceError, job_policy_for_operation,
};
use axon_jobs::backend::JobKind;

mod unified_ops;
use unified_ops::box_send_sync;
pub use unified_ops::{
    cancel_unified_job, cleanup_unified_jobs, clear_unified_jobs, list_unified_jobs,
    recover_unified_jobs, retry_unified_job, unified_job_artifacts, unified_job_events,
    unified_job_status,
};

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
            // No caller identity is available at this call site — every real
            // caller (search/system/memory job-tracking helpers, see
            // `crate::search::job_tracking`, `crate::system::job_tracking`,
            // `crate::memory::job_tracking`) invokes `enqueue_operation`
            // through a generic signature carrying only an `OperationKind` +
            // JSON payload, with no `AuthSnapshot`/`CallerContext` threaded
            // through. This is a genuinely internal/system-triggered
            // bookkeeping path, not an oversight — see
            // docs/pipeline-unification/runtime/auth-contract.md.
            auth_snapshot: AuthSnapshot::trusted_system("runtime"),
            config_snapshot_id: Some(ConfigSnapshotId::new("runtime")),
            requirements: MetadataMap::new(),
            result_schema: Some(result_schema_for_operation(operation).to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .map_err(|error| Box::<dyn Error>::from(error.message))?;
    Ok(Some(descriptor))
}

/// Transition a freshly created job from `Queued` to `Running`.
///
/// Callers that execute a job-backed operation synchronously in the same
/// call (foreground mode) must call this immediately after
/// [`enqueue_operation`] and before running the operation — the state
/// machine only allows the terminal transition in [`complete_operation_job`]
/// from `Running`, never directly from `Queued`.
pub async fn start_operation_job(
    service_context: &ServiceContext,
    descriptor: &JobDescriptor,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let store = service_context
        .job_store()
        .ok_or_else(|| box_send_sync("unified job store is not available"))?;
    store
        .update_status(JobStatusUpdate {
            job_id: descriptor.job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Preparing,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .map_err(|error| box_send_sync(error.message))?;
    Ok(())
}

/// Mark a job created by [`enqueue_operation`] terminal — `Completed` on
/// `Ok`, `Failed` on `Err` — from the operation's own outcome.
///
/// Job-tracking failures here are surfaced to the caller (so a broken jobs DB
/// is observable) but never substitute for the operation's real result —
/// callers should log-and-continue on error rather than let this mask a
/// successful operation.
pub async fn complete_operation_job(
    service_context: &ServiceContext,
    descriptor: &JobDescriptor,
    outcome: Result<(), String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let store = service_context
        .job_store()
        .ok_or_else(|| box_send_sync("unified job store is not available"))?;
    let (status, error) = match outcome {
        Ok(()) => (LifecycleStatus::Completed, None),
        Err(message) => (
            LifecycleStatus::Failed,
            Some(SourceError {
                code: "job.operation_failed".to_string(),
                severity: Severity::Failed,
                message,
                source_item_key: None,
                retryable: false,
                provider_id: None,
                cause: None,
            }),
        ),
    };
    store
        .update_status(JobStatusUpdate {
            job_id: descriptor.job_id,
            source_id: None,
            status,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error,
        })
        .await
        .map_err(|error| box_send_sync(error.message))?;
    Ok(())
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
