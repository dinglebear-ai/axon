//! Unified job tracking for the `research` service entrypoint.
//!
//! `research`/`research_with_context` (see `synthesis.rs`) run synchronously
//! today — the CLI has no `--wait` knob for `axon research <query>`; it
//! always blocks in the calling task until synthesis completes (see
//! `crates/axon-cli/src/commands/research.rs::run_research`). Per
//! `job_policy_for_operation`, `OperationKind::Research` is unconditionally
//! `JobPolicy::JobBacked`, so even a `JobExecutionMode::Foreground` call
//! still creates a real job record — this wrapper enqueues via
//! `crate::jobs::enqueue_operation` and then drives the
//! `Queued -> Running -> Completed/Failed` transitions directly against the
//! unified `JobStore`, mirroring the (not-yet-landed-here)
//! `start_operation_job`/`complete_operation_job` generic helpers used by
//! other job-backed operations (e.g. memory compaction). Job-tracking
//! failures are logged and never mask the research operation's real result.

use std::future::Future;

use axon_api::source::{
    JobDescriptor, JobExecutionMode, JobStatusUpdate, LifecycleStatus, OperationKind,
    PipelinePhase, Severity, SourceError,
};

use crate::context::ServiceContext;

/// Wrap a job-backed research operation with unified job tracking: create a
/// job on enqueue, transition it to `Running` before executing (the state
/// machine rejects `Queued -> Completed` directly), then mark it
/// `Completed`/`Failed` from `op`'s own outcome.
pub(super) async fn track_research_job<T, E, Fut>(
    ctx: &ServiceContext,
    request_json: serde_json::Value,
    op: impl FnOnce() -> Fut,
) -> Result<T, E>
where
    E: std::fmt::Display,
    Fut: Future<Output = Result<T, E>>,
{
    let descriptor = crate::jobs::enqueue_operation(
        ctx,
        OperationKind::Research,
        JobExecutionMode::Foreground,
        request_json,
    )
    .await
    .ok()
    .flatten();

    if let Some(descriptor) = &descriptor {
        mark_running(ctx, descriptor).await;
    }

    let result = op().await;

    if let Some(descriptor) = descriptor {
        let outcome_message = result.as_ref().err().map(ToString::to_string);
        mark_terminal(ctx, &descriptor, outcome_message).await;
    }

    result
}

/// Transition a just-created research job from `Queued` to `Running`.
async fn mark_running(ctx: &ServiceContext, descriptor: &JobDescriptor) {
    let Some(store) = ctx.job_store() else {
        return;
    };
    if let Err(error) = store
        .update_status(JobStatusUpdate {
            job_id: descriptor.id,
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
    {
        tracing::warn!(
            job_id = %descriptor.id.0,
            error = %error,
            "research: failed to record running job status"
        );
    }
}

/// Transition a research job to its terminal status: `Completed` when
/// `error_message` is `None`, `Failed` (carrying the message) otherwise.
async fn mark_terminal(
    ctx: &ServiceContext,
    descriptor: &JobDescriptor,
    error_message: Option<String>,
) {
    let Some(store) = ctx.job_store() else {
        return;
    };
    let (status, error) = match error_message {
        None => (LifecycleStatus::Completed, None),
        Some(message) => (
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
    if let Err(error) = store
        .update_status(JobStatusUpdate {
            job_id: descriptor.id,
            source_id: None,
            status,
            phase: PipelinePhase::Preparing,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error,
        })
        .await
    {
        tracing::warn!(
            job_id = %descriptor.id.0,
            error = %error,
            "research: failed to record terminal job status"
        );
    }
}

#[cfg(test)]
#[path = "job_tracking_tests.rs"]
mod tests;
