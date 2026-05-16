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
