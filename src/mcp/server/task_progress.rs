use super::AxonMcpServer;
use super::task_id::{kind_name, task_id_for};
use crate::jobs::backend::JobKind;
use crate::jobs::status::JobStatus;
use crate::services::runtime::ServiceJobRuntime;
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
        let mapped = map_job_progress(kind, &status, job.result_json.as_ref());
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
        JobKind::Crawl => count_progress(value, "pages_crawled", "pages_discovered", "crawling")
            .unwrap_or(MappedProgress {
                progress: 0.0,
                total: None,
                message: "crawling",
            }),
        JobKind::Embed => count_progress(value, "docs_embedded", "docs_total", "embedding")
            .unwrap_or(MappedProgress {
                progress: 0.0,
                total: None,
                message: "embedding",
            }),
        JobKind::Extract => MappedProgress {
            progress: 0.0,
            total: None,
            message: "running",
        },
        JobKind::Ingest => {
            let message = allowlisted_phase(value.get("phase").and_then(Value::as_str));
            count_progress(value, "files_done", "files_total", message)
                .or_else(|| count_progress(value, "tasks_done", "tasks_total", message))
                .or_else(|| count_progress(value, "chunks_embedded", "chunks_total", message))
                .unwrap_or(MappedProgress {
                    progress: 0.0,
                    total: None,
                    message,
                })
        }
    }
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
        JobKind::Crawl => "crawling",
        JobKind::Embed => "embedding",
        JobKind::Extract => "running",
        JobKind::Ingest => "ingesting",
    }
}

fn allowlisted_phase(phase: Option<&str>) -> &'static str {
    match phase {
        Some("cloning") => "ingesting",
        Some("fetching") => "ingesting",
        Some("indexing") => "ingesting",
        Some("embedding") => "embedding",
        Some("finalizing") => "finalizing",
        Some("completed") => "completed",
        _ => "ingesting",
    }
}

#[cfg(test)]
#[path = "task_progress_tests.rs"]
mod tests;
