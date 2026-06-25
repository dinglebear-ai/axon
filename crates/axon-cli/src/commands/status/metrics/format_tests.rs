use super::*;
use chrono::Duration;

#[test]
fn job_runtime_text_reports_running_elapsed_from_started_at() {
    let started = Utc::now() - Duration::seconds(125);
    let updated = Utc::now();
    let value = job_runtime_text("running", Some(&started), None, &updated);
    assert!(
        value == "2m5s" || value == "2m6s",
        "expected '2m5s' or '2m6s', got '{value}'"
    );
}

#[test]
fn job_runtime_text_reports_completed_duration_from_start_finish() {
    let started = Utc::now() - Duration::seconds(3700);
    let finished = Utc::now();
    let updated = finished;
    let value = job_runtime_text("completed", Some(&started), Some(&finished), &updated);
    assert_eq!(value, "1h1m");
}
