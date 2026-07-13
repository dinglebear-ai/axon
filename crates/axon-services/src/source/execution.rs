use axon_api::source::{AuthSnapshot, JobId, JobPriority, SourceRequest};

#[derive(Debug, Clone)]
pub(crate) struct SourceExecutionContext {
    pub(crate) existing_job_id: Option<JobId>,
    pub(crate) auth_snapshot: Option<AuthSnapshot>,
    pub(crate) priority: JobPriority,
    pub(crate) idempotency_key: Option<String>,
}

impl SourceExecutionContext {
    pub(crate) fn inline(request: SourceRequest, auth_snapshot: Option<AuthSnapshot>) -> Self {
        Self {
            existing_job_id: None,
            auth_snapshot,
            priority: request.execution.priority,
            idempotency_key: request.idempotency_key,
        }
    }

    pub(crate) fn existing_job(
        job_id: JobId,
        request: SourceRequest,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Self {
        Self {
            existing_job_id: Some(job_id),
            auth_snapshot,
            priority: request.execution.priority,
            idempotency_key: request.idempotency_key,
        }
    }
}
