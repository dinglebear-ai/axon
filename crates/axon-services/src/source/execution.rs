use axon_api::source::{AuthSnapshot, JobId, JobPriority, SourceRequest};

use super::foreground_progress::ForegroundProgressSender;

#[derive(Debug, Clone)]
pub(crate) struct SourceExecutionContext {
    pub(crate) existing_job_id: Option<JobId>,
    pub(crate) auth_snapshot: Option<AuthSnapshot>,
    pub(crate) priority: JobPriority,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) attempt: u32,
    pub(crate) foreground: Option<ForegroundProgressSender>,
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
        }
    }
}
