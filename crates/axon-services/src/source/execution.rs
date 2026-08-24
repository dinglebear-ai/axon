use axon_api::source::{AuthSnapshot, JobId, JobPriority, SourceRequest};
use tokio_util::sync::CancellationToken;

use super::foreground_progress::ForegroundProgressSender;

#[derive(Debug, Clone)]
pub(crate) struct SourceExecutionContext {
    pub(crate) existing_job_id: Option<JobId>,
    pub(crate) auth_snapshot: Option<AuthSnapshot>,
    pub(crate) priority: JobPriority,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) attempt: u32,
    pub(crate) foreground: Option<ForegroundProgressSender>,
    /// Cooperative cancellation for worker-executed runs. When set, the
    /// executor resolves to an error at its cancellation checkpoint instead
    /// of relying on the caller dropping the pipeline future, so
    /// failed-generation cleanup (vector cleanup + `fail_generation`) still
    /// runs for an uncommitted generation (2026-08-23 adversarial pipeline
    /// review, M3).
    pub(crate) cancellation: Option<CancellationToken>,
}

impl SourceExecutionContext {
    pub(crate) fn inline(request: SourceRequest, auth_snapshot: Option<AuthSnapshot>) -> Self {
        Self {
            existing_job_id: None,
            auth_snapshot,
            priority: request.execution.priority,
            idempotency_key: request.idempotency_key,
            attempt: 1,
            foreground: None,
            cancellation: None,
        }
    }

    pub(crate) fn inline_with_progress(
        request: SourceRequest,
        auth_snapshot: Option<AuthSnapshot>,
        foreground: ForegroundProgressSender,
    ) -> Self {
        let mut execution = Self::inline(request, auth_snapshot);
        execution.foreground = Some(foreground);
        execution
    }

    pub(crate) fn existing_job(
        job_id: JobId,
        request: SourceRequest,
        auth_snapshot: Option<AuthSnapshot>,
        attempt: u32,
    ) -> Self {
        Self {
            existing_job_id: Some(job_id),
            auth_snapshot,
            priority: request.execution.priority,
            idempotency_key: request.idempotency_key,
            attempt,
            foreground: None,
            cancellation: None,
        }
    }

    #[must_use]
    pub(crate) fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = Some(cancellation);
        self
    }
}
