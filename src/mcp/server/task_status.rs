use super::task_id::task_id_for;
use crate::jobs::backend::JobKind;
use crate::jobs::status::JobStatus;
use crate::services::types::ServiceJob;
use rmcp::model::{GetTaskPayloadResult, Task, TaskStatus};
use serde_json::json;

pub(super) const TASK_POLL_INTERVAL_MS: u64 = 5_000;

pub(super) fn task_from_job(kind: JobKind, job: &ServiceJob) -> Task {
    let mut task = Task::new(
        task_id_for(kind, job.id),
        task_status(job.status_enum()),
        job.created_at.to_rfc3339(),
        job.updated_at.to_rfc3339(),
    )
    .with_poll_interval(TASK_POLL_INTERVAL_MS);

    if let Some(message) = status_message(job.status_enum()) {
        task = task.with_status_message(message);
    }
    task
}

pub(super) fn task_result_payload(kind: JobKind, job: &ServiceJob) -> GetTaskPayloadResult {
    GetTaskPayloadResult::new(json!({
        "task_id": task_id_for(kind, job.id),
        "job_id": job.id,
        "kind": super::task_id::kind_name(kind),
        "status": job.status,
        "completed": job.status_enum() == JobStatus::Completed,
        "terminal": matches!(
            job.status_enum(),
            JobStatus::Completed | JobStatus::Failed | JobStatus::Canceled
        ),
        "created_at": job.created_at,
        "updated_at": job.updated_at,
        "started_at": job.started_at,
        "finished_at": job.finished_at,
    }))
}

fn task_status(status: JobStatus) -> TaskStatus {
    match status {
        JobStatus::Pending | JobStatus::Running => TaskStatus::Working,
        JobStatus::Completed => TaskStatus::Completed,
        JobStatus::Failed => TaskStatus::Failed,
        JobStatus::Canceled => TaskStatus::Cancelled,
    }
}

fn status_message(status: JobStatus) -> Option<&'static str> {
    match status {
        JobStatus::Pending => Some("queued"),
        JobStatus::Running => Some("running"),
        JobStatus::Completed => Some("completed"),
        JobStatus::Failed => Some("failed"),
        JobStatus::Canceled => Some("cancelled"),
    }
}

#[cfg(test)]
#[path = "task_status_tests.rs"]
mod tests;
