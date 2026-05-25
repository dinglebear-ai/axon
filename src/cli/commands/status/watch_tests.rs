use super::*;

#[test]
fn is_active_recognizes_all_running_states() {
    for s in &["running", "pending", "processing", "scraping", "claimed"] {
        assert!(is_active(s), "{s} should be active");
    }
}

#[test]
fn is_active_rejects_terminal_states() {
    for s in &["completed", "failed", "canceled", "cancelled", "error"] {
        assert!(!is_active(s), "{s} should not be active");
    }
}

fn make_job(url: Option<&str>, source_type: Option<&str>, target: Option<&str>) -> ServiceJob {
    ServiceJob {
        id: uuid::Uuid::nil(),
        status: "running".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        started_at: None,
        finished_at: None,
        error_text: None,
        url: url.map(String::from),
        source_type: source_type.map(String::from),
        target: target.map(String::from),
        urls_json: None,
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
    assert_eq!(format_subject(&j), uuid::Uuid::nil().to_string());
}
