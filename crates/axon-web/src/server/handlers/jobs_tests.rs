use super::*;
use axon_api::job_progress::JobPhase;

#[test]
fn job_family_maps_generic_async_families() {
    assert_eq!(job_family(JobKind::Embed), Some(JobFamily::Embed));
    assert_eq!(job_family(JobKind::Extract), Some(JobFamily::Extract));
    assert_eq!(job_family(JobKind::Ingest), Some(JobFamily::Ingest));
}

#[test]
fn job_family_returns_none_for_crawl() {
    // Crawl carries a richer client-side snapshot, not the generic progress
    // shape — so the status handler must omit `progress` for it.
    assert_eq!(job_family(JobKind::Crawl), None);
}

#[test]
fn job_status_response_serializes_job_and_progress() {
    // The wire contract is `{ "job": {...}, "progress": {...} | null }`.
    let resp = JobStatusResponse {
        job: json!({ "id": "abc", "status": "running" }),
        progress: Some(JobProgress::derive(
            JobFamily::Ingest,
            "running",
            Some(&json!({ "tasks_done": 1, "tasks_total": 2 })),
            None,
        )),
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["job"]["status"], "running");
    assert_eq!(v["progress"]["family"], "ingest");
    assert_eq!(v["progress"]["phase"], "running");
    assert_eq!(v["progress"]["percent"], 50.0);

    // Crawl-style response carries an explicit null progress.
    let resp = JobStatusResponse {
        job: json!({ "id": "xyz" }),
        progress: None,
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert!(v["progress"].is_null());
}

#[test]
fn job_phase_terminal_classification_is_stable() {
    // Guards the contract the palette poll loop relies on to stop polling.
    assert!(JobPhase::Done.is_terminal());
    assert!(JobPhase::Failed.is_terminal());
    assert!(JobPhase::Canceled.is_terminal());
    assert!(!JobPhase::Running.is_terminal());
    assert!(!JobPhase::Pending.is_terminal());
}
