use crate::crates::core::logging::log_warn;
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn phase_reporter_sends_progress() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter
            .report(serde_json::json!({
                "phase": "fetching_issues",
                "issues_fetched": 42,
            }))
            .await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["phase"], "fetching_issues");
        assert_eq!(msg["issues_fetched"], 42);
    }

    #[tokio::test]
    async fn phase_reporter_none_is_noop() {
        let reporter = PhaseReporter::new(None);
        reporter.report(serde_json::json!({"phase": "test"})).await;
    }

    #[tokio::test]
    async fn phase_reporter_report_phase_sends_phase_only() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report_phase("cloning").await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["phase"], "cloning");
    }

    #[tokio::test]
    async fn phase_reporter_arbitrary_source_phases() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter.report_phase("downloading_transcript").await;
        reporter.report_phase("fetching_subreddit").await;
        reporter.report_phase("scanning_sessions").await;

        let msg1 = rx.recv().await.unwrap();
        assert_eq!(msg1["phase"], "downloading_transcript");
        let msg2 = rx.recv().await.unwrap();
        assert_eq!(msg2["phase"], "fetching_subreddit");
        let msg3 = rx.recv().await.unwrap();
        assert_eq!(msg3["phase"], "scanning_sessions");
    }

    #[tokio::test]
    async fn progress_reporter_sends_all_phases() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(64);
        let reporter = PhaseReporter::new(Some(tx));

        let phases = [
            "cloning",
            "enumerating_files",
            "collecting_files",
            "embedding_batch",
            "embedded_files",
            "fetching_issues",
            "embedding_issues",
            "fetching_prs",
            "embedding_prs",
            "completed",
        ];

        for phase in &phases {
            reporter.report_phase(phase).await;
        }
        // Drop reporter (and thus the sender) so the receiver terminates.
        drop(reporter);

        let mut received = Vec::new();
        while let Some(msg) = rx.recv().await {
            received.push(msg["phase"].as_str().unwrap_or("").to_string());
        }

        assert_eq!(received.len(), phases.len());
        assert_eq!(received[0], "cloning");
        assert_eq!(received.last().unwrap(), "completed");
    }

    #[tokio::test]
    async fn progress_reporter_sends_rich_payloads() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);
        let reporter = PhaseReporter::new(Some(tx));

        reporter
            .report(serde_json::json!({
                "phase": "fetching_issues",
                "issues_fetched": 42,
                "issues_page": 2,
                "tasks_done": 3,
                "tasks_total": 5,
            }))
            .await;
        drop(reporter);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg["phase"], "fetching_issues");
        assert_eq!(msg["issues_fetched"], 42);
        assert_eq!(msg["issues_page"], 2);
        assert_eq!(msg["tasks_done"], 3);
        assert_eq!(msg["tasks_total"], 5);
    }
}
