use crate::crates::core::logging::log_warn;
use tokio::sync::mpsc;

/// Lightweight progress reporter that wraps an optional mpsc sender.
///
/// Designed to be passed by reference into ingest sub-tasks. When the sender
/// is `None` (e.g. synchronous `--wait` mode or tests), all calls are no-ops.
///
/// **No central phase enum.** Each ingest source defines its own phase
/// constants as `&str` in its own module. This keeps sources fully decoupled.
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
        if let Some(tx) = &self.tx {
            if let Err(e) = tx.send(progress).await {
                log_warn(&format!("progress_send_failed err={e}"));
            }
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
}
