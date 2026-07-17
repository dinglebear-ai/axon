use super::AxonMcpServer;
use super::task_id::{kind_name, task_id_for};
use axon_api::{job_status::JobStatus, source::JobKind};
use axon_services::runtime::ServiceJobRuntime;
use rmcp::model::{ProgressNotificationParam, ProgressToken};
use rmcp::{Peer, RoleServer};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

pub(super) const NOTIFIER_READ_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(super::task_status::TASK_POLL_INTERVAL_MS);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct MappedProgress {
    pub progress: f64,
    pub total: Option<f64>,
    pub message: &'static str,
}

impl MappedProgress {
    pub fn into_notification(self, progress_token: ProgressToken) -> ProgressNotificationParam {
        let mut param = ProgressNotificationParam::new(progress_token, self.progress)
            .with_message(self.message);
        if let Some(total) = self.total {
            param = param.with_total(total);
        }
        param
    }
}

pub(super) async fn start_progress_notifier(
    server: &AxonMcpServer,
    kind: JobKind,
    job_id: Uuid,
    progress_token: Option<ProgressToken>,
    peer: Peer<RoleServer>,
) {
    let Some(progress_token) = progress_token else {
        tracing::debug!(
            task_id = %task_id_for(kind, job_id),
            kind = kind_name(kind),
            has_progress_token = false,
            "mcp.task.progress.start"
        );
        return;
    };
    let task_id = task_id_for(kind, job_id);
    let notifier_key = format!("{task_id}:{progress_token:?}");
    let ctx = match server.base_service_context().await {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(
                task_id = %task_id,
                kind = kind_name(kind),
                error = %e,
                "mcp.task.progress.stop"
            );
            return;
        }
    };

    {
        let mut notifiers = server.progress_notifiers.lock().await;
        if notifiers
            .get(&notifier_key)
            .is_some_and(|handle| !handle.is_finished())
        {
            return;
        }
        if notifiers
            .get(&notifier_key)
            .is_some_and(tokio::task::JoinHandle::is_finished)
        {
            notifiers.remove(&notifier_key);
        }
    }

    let jobs = ctx.jobs.clone();
    let progress_notifiers = server.progress_notifiers.clone();
    let cleanup_key = notifier_key.clone();
    let handle = tokio::spawn(async move {
        run_progress_notifier(kind, job_id, task_id, progress_token, peer, jobs).await;
        progress_notifiers.lock().await.remove(&cleanup_key);
    });
    let mut notifiers = server.progress_notifiers.lock().await;
    if notifiers
        .get(&notifier_key)
        .is_some_and(|existing| !existing.is_finished())
    {
        handle.abort();
        return;
    }
    notifiers.insert(notifier_key, handle);
}

async fn run_progress_notifier(
    kind: JobKind,
    job_id: Uuid,
    task_id: String,
    progress_token: ProgressToken,
    peer: Peer<RoleServer>,
    jobs: Arc<dyn ServiceJobRuntime>,
) {
    tracing::info!(
        task_id = %task_id,
        kind = kind_name(kind),
        has_progress_token = true,
        "mcp.task.progress.start"
    );
    let mut last_fingerprint = String::new();
    loop {
        tokio::time::sleep(NOTIFIER_READ_INTERVAL).await;
        let job = match jobs.job_status(kind, job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                tracing::debug!(
                    task_id = %task_id,
                    kind = kind_name(kind),
                    reason = "not_found",
                    "mcp.task.progress.stop"
                );
                return;
            }
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    kind = kind_name(kind),
                    error = %e,
                    "mcp.task.progress.stop"
                );
                return;
            }
        };
        let status = job.status_enum();
        let mapped = map_job_progress(
            kind,
            &status,
            progress_metrics_for_status(
                &status,
                job.progress_json.as_ref(),
                job.result_json.as_ref(),
            ),
        );
        let fingerprint = progress_fingerprint(&status, job.updated_at, &mapped);
        if fingerprint != last_fingerprint {
            last_fingerprint = fingerprint;
            let has_total = mapped.total.is_some();
            let notification = mapped.into_notification(progress_token.clone());
            tracing::debug!(
                task_id = %task_id,
                kind = kind_name(kind),
                progress = notification.progress,
                has_total,
                status = %status.as_str(),
                "mcp.task.progress.emit"
            );
            if peer.notify_progress(notification).await.is_err() {
                tracing::debug!(
                    task_id = %task_id,
                    kind = kind_name(kind),
                    reason = "send_failed",
                    "mcp.task.progress.stop"
                );
                return;
            }
        }
        if !status.is_active() {
            tracing::debug!(
                task_id = %task_id,
                kind = kind_name(kind),
                reason = "terminal",
                "mcp.task.progress.stop"
            );
            return;
        }
    }
}

pub(super) fn map_job_progress(
    kind: JobKind,
    status: &JobStatus,
    result_json: Option<&Value>,
) -> MappedProgress {
    match status {
        JobStatus::Pending => MappedProgress {
            progress: 0.0,
            total: None,
            message: "queued",
        },
        JobStatus::Completed => terminal("completed"),
        JobStatus::Failed => terminal("failed"),
        JobStatus::Canceled => terminal("cancelled"),
        JobStatus::Running => map_running_progress(kind, result_json),
        JobStatus::Unknown(_) => terminal("unknown status"),
    }
}

pub(super) fn progress_metrics_for_status<'a>(
    status: &JobStatus,
    progress_json: Option<&'a Value>,
    result_json: Option<&'a Value>,
) -> Option<&'a Value> {
    if status.clone().is_active() {
        usable_progress_json(progress_json).or(result_json)
    } else {
        result_json
    }
}

fn usable_progress_json(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| {
        !(value.get("degraded").and_then(Value::as_bool) == Some(true)
            && value.get("field").and_then(Value::as_str) == Some("progress_json"))
    })
}

pub(super) fn progress_fingerprint(
    status: &JobStatus,
    updated_at: chrono::DateTime<chrono::Utc>,
    mapped: &MappedProgress,
) -> String {
    format!(
        "{}:{}:{}:{:?}:{}",
        status.as_str(),
        updated_at.timestamp_millis(),
        mapped.progress,
        mapped.total,
        mapped.message
    )
}

fn terminal(message: &'static str) -> MappedProgress {
    MappedProgress {
        progress: 1.0,
        total: Some(1.0),
        message,
    }
}

fn map_running_progress(kind: JobKind, result_json: Option<&Value>) -> MappedProgress {
    let Some(value) = result_json.and_then(Value::as_object) else {
        return MappedProgress {
            progress: 0.0,
            total: None,
            message: running_message(kind),
        };
    };

    match kind {
        JobKind::Source => source_progress(value),
        JobKind::Extract => MappedProgress {
            progress: 0.0,
            total: None,
            message: "running",
        },
        _ => MappedProgress {
            progress: 0.0,
            total: None,
            message: running_message(kind),
        },
    }
}

fn source_progress(object: &serde_json::Map<String, Value>) -> MappedProgress {
    let message = allowlisted_phase(object.get("phase").and_then(Value::as_str));
    count_progress(object, "pages_crawled", "pages_discovered", "indexing")
        .or_else(|| count_progress(object, "docs_embedded", "docs_total", "embedding"))
        .or_else(|| count_progress(object, "documents_done", "documents_total", "embedding"))
        .or_else(|| count_progress(object, "items_done", "items_total", message))
        .or_else(|| count_progress(object, "files_done", "files_total", message))
        .or_else(|| count_progress(object, "tasks_done", "tasks_total", message))
        .or_else(|| count_progress(object, "chunks_embedded", "chunks_total", message))
        .or_else(|| count_progress(object, "chunks_done", "chunks_total", message))
        .unwrap_or(MappedProgress {
            progress: 0.0,
            total: None,
            message,
        })
}

fn count_progress(
    object: &serde_json::Map<String, Value>,
    done_key: &str,
    total_key: &str,
    message: &'static str,
) -> Option<MappedProgress> {
    let progress = number(object.get(done_key)?)?;
    let total = object.get(total_key).and_then(number).filter(|v| *v > 0.0);
    Some(MappedProgress {
        progress,
        total,
        message,
    })
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
}

fn running_message(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Source => "indexing",
        JobKind::Extract => "running",
        JobKind::Watch => "watching",
        JobKind::Map => "mapping",
        JobKind::Research => "researching",
        JobKind::Ask => "answering",
        JobKind::Query => "querying",
        JobKind::Retrieve => "retrieving",
        JobKind::Memory => "updating memory",
        JobKind::Graph => "graphing",
        JobKind::Prune => "pruning",
        JobKind::ProviderProbe => "probing",
        JobKind::Reset => "resetting",
    }
}

fn allowlisted_phase(phase: Option<&str>) -> &'static str {
    match phase {
        Some("cloning") => "indexing",
        Some("fetching") => "indexing",
        Some("indexing") => "indexing",
        Some("embedding") => "embedding",
        Some("finalizing") => "finalizing",
        Some("completed") => "completed",
        _ => "indexing",
    }
}

pub(super) fn structured_source_progress(value: Option<&Value>) -> Option<Value> {
    let object = value?.as_object()?;
    let counts = object
        .get("counts")
        .filter(|value| value.is_object())
        .cloned()
        .or_else(|| flat_stage_counts(object));
    let current = object
        .get("current")
        .filter(|value| value.is_object())
        .cloned();
    let warnings = diagnostic_array(object, "warnings", "warning");
    let errors = diagnostic_array(object, "errors", "error");

    if counts.is_none() && current.is_none() && warnings.is_empty() && errors.is_empty() {
        return None;
    }

    Some(serde_json::json!({
        "counts": counts,
        "current": current,
        "warnings": warnings,
        "errors": errors,
    }))
}

fn flat_stage_counts(object: &serde_json::Map<String, Value>) -> Option<Value> {
    const COUNT_KEYS: [&str; 8] = [
        "items_total",
        "items_done",
        "documents_total",
        "documents_done",
        "chunks_total",
        "chunks_done",
        "bytes_total",
        "bytes_done",
    ];
    let counts = COUNT_KEYS
        .into_iter()
        .filter_map(|key| {
            object
                .get(key)
                .cloned()
                .map(|value| (key.to_string(), value))
        })
        .collect::<serde_json::Map<_, _>>();
    (!counts.is_empty()).then_some(Value::Object(counts))
}

fn diagnostic_array(
    object: &serde_json::Map<String, Value>,
    array_key: &str,
    singular_key: &str,
) -> Vec<Value> {
    let mut diagnostics = object
        .get(array_key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(diagnostic) = object.get(singular_key).filter(|value| value.is_object()) {
        diagnostics.push(diagnostic.clone());
    }
    diagnostics
}

#[cfg(test)]
#[path = "task_progress_tests.rs"]
mod tests;
