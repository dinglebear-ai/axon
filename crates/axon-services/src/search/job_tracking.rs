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

use axon_api::source::{AuthSnapshot, JobExecutionMode, JobPriority, OperationKind};

use crate::context::ServiceContext;

/// Wrap a job-backed research operation with unified job tracking: create a
/// job on enqueue, transition it to `Running` before executing (the state
/// machine rejects `Queued -> Completed` directly), then mark it
/// `Completed`/`Failed` from `op`'s own outcome.
pub(super) async fn track_research_job<T, E, Fut>(
    ctx: &ServiceContext,
    request_json: serde_json::Value,
    auth_snapshot: Option<AuthSnapshot>,
    op: impl FnOnce() -> Fut,
) -> Result<T, E>
where
    E: std::fmt::Display,
    Fut: Future<Output = Result<T, E>>,
{
    let descriptor = crate::jobs::enqueue_operation_with_context(
        ctx,
        OperationKind::Research,
        JobExecutionMode::Foreground,
        request_json,
        JobPriority::Normal,
        auth_snapshot.unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
    )
    .await
    .ok()
    .flatten();

    if let Some(descriptor) = &descriptor
        && let Err(error) = crate::jobs::start_operation_job(ctx, descriptor).await
    {
        tracing::warn!(job_id = %descriptor.job_id.0, %error, "research: failed to record running job status");
    }

    let result = op().await;

    if let Some(descriptor) = descriptor {
        let outcome = result.as_ref().map(|_| ()).map_err(ToString::to_string);
        if let Err(error) = crate::jobs::complete_operation_job(ctx, &descriptor, outcome).await {
            tracing::warn!(job_id = %descriptor.job_id.0, %error, "research: failed to record terminal job status");
        }
    }

    result
}

#[cfg(test)]
#[path = "job_tracking_tests.rs"]
mod tests;
