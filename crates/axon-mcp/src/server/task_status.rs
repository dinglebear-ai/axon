use super::task_id::task_id_for;
use super::task_progress::structured_source_progress;
use axon_api::{job_status::JobStatus, source::JobKind};
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};
use axon_services::types::ServiceJob;
use rmcp::model::{DetailedTask, MetaObject, Task, TaskPayload, TaskStatus};
use serde_json::{Map, Value, json};

const RESULT_JSON_MAX_BYTES: usize = 64 * 1024;

pub(super) const TASK_POLL_INTERVAL_MS: u64 = 5_000;

pub(super) fn task_from_job(kind: JobKind, job: &ServiceJob) -> Task {
    let mut task = Task::new(
        task_id_for(kind, job.id),
        task_status(&job.status_enum()),
        job.created_at.to_rfc3339(),
        job.updated_at.to_rfc3339(),
    )
    .with_poll_interval_ms(TASK_POLL_INTERVAL_MS);

    if let Some(message) = status_message(&job.status_enum()) {
        task = task.with_status_message(message);
    }
    task
}

/// Build the SEP-2663 [`DetailedTask`] for a job.
///
/// rmcp 3.x folds the old `tasks/result` payload into the task itself: a
/// terminal job carries its result (or error) inline in [`TaskPayload`],
/// so [`task_result_payload`] is now the body of the `completed`/`failed`
/// variants rather than a separate `tasks/result` response.
pub(super) fn detailed_task_from_job(kind: JobKind, job: &ServiceJob) -> DetailedTask {
    let task = task_from_job(kind, job);
    let payload = match job.status_enum() {
        JobStatus::Pending | JobStatus::Running => TaskPayload::Working,
        JobStatus::Canceled => TaskPayload::Cancelled,
        JobStatus::Completed => TaskPayload::Completed {
            result: task_result_payload(kind, job),
        },
        JobStatus::Failed | JobStatus::Unknown(_) => TaskPayload::Failed {
            error: task_result_payload(kind, job),
        },
    };
    DetailedTask::new(task, payload)
}

fn task_result_payload(kind: JobKind, job: &ServiceJob) -> Map<String, Value> {
    let progress = task_progress_value(kind, job);
    let payload = json!({
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
        "progress": progress,
        "created_at": job.created_at,
        "updated_at": job.updated_at,
        "started_at": job.started_at,
        "finished_at": job.finished_at,
    });
    match payload {
        Value::Object(map) => map,
        // `json!` with an object literal is always an object.
        other => {
            let mut map = Map::new();
            map.insert("payload".to_string(), other);
            map
        }
    }
}

pub(super) fn task_meta_from_job(kind: JobKind, job: &ServiceJob) -> Option<MetaObject> {
    let progress = task_progress_value(kind, job)?;
    let mut meta = MetaObject::new();
    meta.insert("axon".to_string(), json!({ "progress": progress }));
    Some(meta)
}

fn task_progress_value(kind: JobKind, job: &ServiceJob) -> Option<Value> {
    if kind != JobKind::Source {
        return None;
    }
    let mut progress = structured_source_progress(job.progress_json.as_ref())?;
    if let Value::Object(object) = &mut progress {
        object.insert("phase".to_string(), json!(job.phase));
        if let Some(source_kind) = job.source_kind {
            object.insert("source_kind".to_string(), json!(source_kind));
        }
    }
    sanitized_bounded_json(&progress, "progress")
}

fn sanitized_result_json(result_json: Option<&Value>) -> Option<Value> {
    sanitized_bounded_json(result_json?, "result_json")
}

fn sanitized_bounded_json(value: &Value, field: &str) -> Option<Value> {
    let value = sanitize_value(value);
    match serde_json::to_vec(&value) {
        Ok(bytes) if bytes.len() <= RESULT_JSON_MAX_BYTES => Some(value),
        Ok(bytes) => Some(json!({
            "truncated": true,
            "reason": format!("{field} exceeded task payload size limit"),
            "bytes": bytes.len(),
            "limit_bytes": RESULT_JSON_MAX_BYTES,
        })),
        Err(_) => Some(json!({
            "truncated": true,
            "reason": format!("{field} could not be serialized"),
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
    axon_core::redact::is_secret_like(key)
}

fn sanitize_string(value: &str) -> String {
    let redacted =
        DefaultRedactor::new().redact_text(value, &RedactionContext::transport_response());
    if redacted != value {
        return redacted;
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
