//! Shared unified-job tracking wrapper for job-backed memory operations.
//!
//! `enqueue_operation`/`start_operation_job`/`complete_operation_job` (in
//! `crate::jobs`) are the transport-neutral primitives; this wrapper is the
//! one place memory operations call them so every job-backed memory action
//! (compaction today, import/other lifecycle ops later) gets the same
//! create -> Running -> Completed/Failed lifecycle without repeating it.

use std::future::Future;

use axon_api::source::{JobExecutionMode, OperationKind};

use crate::context::ServiceContext;

/// Wrap a job-backed memory operation with unified job tracking: create a
/// job on enqueue, transition it to `Running` before executing (the state
/// machine rejects `Queued -> Completed` directly), then mark it
/// `Completed`/`Failed` from `op`'s own outcome. Job-tracking failures are
/// logged and never mask the operation's real result.
pub(super) async fn track_operation_job<T, Fut>(
    ctx: &ServiceContext,
    operation: OperationKind,
    request_json: serde_json::Value,
    op: impl FnOnce() -> Fut,
) -> anyhow::Result<T>
where
    Fut: Future<Output = anyhow::Result<T>>,
{
    let descriptor =
        crate::jobs::enqueue_operation(ctx, operation, JobExecutionMode::Foreground, request_json)
            .await
            .ok()
            .flatten();

    if let Some(descriptor) = &descriptor
        && let Err(error) = crate::jobs::start_operation_job(ctx, descriptor).await
    {
        tracing::warn!(
            job_id = %descriptor.job_id.0,
            error = %error,
            operation = ?operation,
            "memory: failed to record running job status"
        );
    }

    let result = op().await;

    if let Some(descriptor) = descriptor {
        let outcome = result.as_ref().map(|_| ()).map_err(|e| e.to_string());
        if let Err(error) = crate::jobs::complete_operation_job(ctx, &descriptor, outcome).await {
            tracing::warn!(
                job_id = %descriptor.job_id.0,
                error = %error,
                operation = ?operation,
                "memory: failed to record terminal job status"
            );
        }
    }

    result
}
