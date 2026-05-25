use super::*;
use uuid::Uuid;

#[test]
fn is_active_recognizes_all_running_states() {
    for s in &["running", "pending"] {
        let job = make_job_with_status(s, None, None, None, None);
        assert!(is_active(&job), "{s} should be active");
    }
}

#[test]
fn is_active_rejects_terminal_states() {
    for s in &["completed", "failed", "canceled", "cancelled", "error"] {
        let job = make_job_with_status(s, None, None, None, None);
        assert!(!is_active(&job), "{s} should not be active");
    }
}

fn make_job(url: Option<&str>, source_type: Option<&str>, target: Option<&str>) -> ServiceJob {
    make_job_with_status("running", url, source_type, target, None)
}

fn make_job_with_status(
    status: &str,
    url: Option<&str>,
    source_type: Option<&str>,
    target: Option<&str>,
    urls_json: Option<serde_json::Value>,
) -> ServiceJob {
    ServiceJob {
        id: Uuid::nil(),
        status: status.to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        started_at: None,
        finished_at: None,
        error_text: None,
        url: url.map(String::from),
        source_type: source_type.map(String::from),
        target: target.map(String::from),
        urls_json,
        result_json: None,
        config_json: None,
        attempt_count: 0,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn format_subject_prefers_url() {
    let j = make_job(Some("https://example.com"), Some("github"), Some("foo/bar"));
    assert_eq!(format_subject(&j), "https://example.com");
}

#[test]
fn format_subject_falls_back_to_source_target_when_no_url() {
    let j = make_job(None, Some("github"), Some("foo/bar"));
    assert_eq!(format_subject(&j), "github: foo/bar");
}

#[test]
fn format_subject_falls_back_to_target_when_no_source_type() {
    let j = make_job(None, None, Some("foo/bar"));
    assert_eq!(format_subject(&j), "foo/bar");
}

#[test]
fn format_subject_falls_back_to_id_when_nothing_else() {
    let j = make_job(None, None, None);
    assert_eq!(format_subject(&j), Uuid::nil().to_string());
}

#[test]
fn format_subject_uses_extract_url_list() {
    let j = make_job_with_status(
        "running",
        None,
        None,
        None,
        Some(serde_json::json!([
            "https://example.com/a",
            "https://example.com/b"
        ])),
    );
    assert_eq!(format_subject(&j), "https://example.com/a (+1 more)");
}
