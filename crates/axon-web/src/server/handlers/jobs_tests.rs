use super::*;
use axon_api::job_progress::JobPhase;

#[test]
fn job_family_maps_generic_async_families() {
    assert_eq!(job_family(JobKind::Source), Some(JobFamily::Source));
    assert_eq!(job_family(JobKind::Extract), Some(JobFamily::Extract));
}

#[test]
fn job_family_returns_none_for_non_progress_jobs() {
    assert_eq!(job_family(JobKind::Watch), None);
}

#[test]
fn job_status_response_serializes_job_and_progress() {
    // The wire contract is `{ "job": {...}, "progress": {...} | null }`.
    let resp = JobStatusResponse {
        job: json!({ "id": "abc", "status": "running" }),
        progress: Some(JobProgress::derive(
            JobFamily::Source,
            "running",
            Some(&json!({ "tasks_done": 1, "tasks_total": 2 })),
            None,
        )),
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["job"]["status"], "running");
    assert_eq!(v["progress"]["family"], "source");
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
