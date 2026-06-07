use super::task_id::task_id_for;
use crate::jobs::backend::JobKind;
use crate::jobs::status::JobStatus;
use crate::services::types::ServiceJob;
use rmcp::model::{GetTaskPayloadResult, Task, TaskStatus};
use serde_json::{Value, json};

const RESULT_JSON_MAX_BYTES: usize = 64 * 1024;

pub(super) const TASK_POLL_INTERVAL_MS: u64 = 5_000;

pub(super) fn task_from_job(kind: JobKind, job: &ServiceJob) -> Task {
    let mut task = Task::new(
        task_id_for(kind, job.id),
        task_status(&job.status_enum()),
        job.created_at.to_rfc3339(),
        job.updated_at.to_rfc3339(),
    )
    .with_poll_interval(TASK_POLL_INTERVAL_MS);

    if let Some(message) = status_message(&job.status_enum()) {
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
        "result_json": sanitized_result_json(job.result_json.as_ref()),
        "created_at": job.created_at,
        "updated_at": job.updated_at,
        "started_at": job.started_at,
        "finished_at": job.finished_at,
    }))
}

fn sanitized_result_json(result_json: Option<&Value>) -> Option<Value> {
    let value = sanitize_value(result_json?);
    match serde_json::to_vec(&value) {
        Ok(bytes) if bytes.len() <= RESULT_JSON_MAX_BYTES => Some(value),
        Ok(bytes) => Some(json!({
            "truncated": true,
            "reason": "result_json exceeded task payload size limit",
            "bytes": bytes.len(),
            "limit_bytes": RESULT_JSON_MAX_BYTES,
        })),
        Err(_) => Some(json!({
            "truncated": true,
            "reason": "result_json could not be serialized",
            "limit_bytes": RESULT_JSON_MAX_BYTES,
        })),
    }
}

fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    if is_sensitive_key(key) {
                        (key.clone(), Value::String("[redacted]".to_string()))
                    } else {
                        (key.clone(), sanitize_value(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_value).collect()),
        Value::String(value) => Value::String(sanitize_string(value)),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("password")
        || lower == "authorization"
        || lower == "api_key"
}

fn sanitize_string(value: &str) -> String {
    if value.contains("://") && value.contains('@') {
        return "[redacted-url]".to_string();
    }
    if value.len() > 4096 {
        let mut truncated = value.chars().take(4096).collect::<String>();
        truncated.push_str("...[truncated]");
        return truncated;
    }
    value.to_string()
}

fn task_status(status: &JobStatus) -> TaskStatus {
    match status {
        JobStatus::Pending | JobStatus::Running => TaskStatus::Working,
        JobStatus::Completed => TaskStatus::Completed,
        JobStatus::Failed => TaskStatus::Failed,
        JobStatus::Canceled => TaskStatus::Cancelled,
        JobStatus::Unknown(_) => TaskStatus::Failed,
    }
}

fn status_message(status: &JobStatus) -> Option<&'static str> {
    match status {
        JobStatus::Pending => Some("queued"),
        JobStatus::Running => Some("running"),
        JobStatus::Completed => Some("completed"),
        JobStatus::Failed => Some("failed"),
        JobStatus::Canceled => Some("cancelled"),
        JobStatus::Unknown(_) => Some("unknown job status"),
    }
}

#[cfg(test)]
#[path = "task_status_tests.rs"]
mod tests;
