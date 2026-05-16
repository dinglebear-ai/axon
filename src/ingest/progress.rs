use crate::core::logging::log_warn;
use tokio::sync::mpsc;

/// Lightweight progress reporter that wraps an optional mpsc sender.
///
/// Designed to be passed by reference into ingest sub-tasks. When the sender
/// is `None` (e.g. synchronous `--wait` mode or tests), all calls are no-ops.
///
/// **No central phase enum.** Each ingest source defines its own phase
/// constants as `&str` in its own module. This keeps sources fully decoupled.
///
/// Common payload keys consumed by status/list renderers:
/// - `phase`: current source-defined phase label.
/// - `chunks_embedded`: cumulative embedded chunk count.
/// - `files_done` / `files_total`: GitHub-style file progress.
/// - `videos_done` / `videos_total`: YouTube playlist/channel progress.
/// - `tasks_done` / `tasks_total`: multi-subtask source progress.
///
/// Providers may add source-specific fields, but should preserve these names
/// for shared CLI/MCP status rendering.
#[derive(Clone)]
pub struct PhaseReporter {
    tx: Option<mpsc::Sender<serde_json::Value>>,
}

impl PhaseReporter {
    pub fn new(tx: Option<mpsc::Sender<serde_json::Value>>) -> Self {
        Self { tx }
    }

    /// A no-op reporter for sources that don't have a progress channel.
    pub fn noop() -> Self {
        Self { tx: None }
    }

    /// Send an arbitrary progress JSON blob.
    pub async fn report(&self, progress: serde_json::Value) {
        let Some(tx) = &self.tx else { return };
        if let Err(e) = tx.try_send(progress) {
            log_warn(&format!("progress_send_dropped err={e}"));
        }
    }

    /// Convenience: send a phase-only update.
    pub async fn report_phase(&self, phase: &str) {
        self.report(serde_json::json!({ "phase": phase })).await;
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
