use super::*;

#[test]
fn watchdog_reclaim_detection_matches_prefix_on_failed_jobs() {
    assert!(is_watchdog_reclaimed_failure(
        "failed",
        Some("watchdog reclaimed stale running ingest job (idle=360s marker=amqp)")
    ));
    assert!(!is_watchdog_reclaimed_failure(
        "error",
        Some("watchdog reclaimed stale running crawl job (idle=361s marker=polling)")
    ));
    assert!(!is_watchdog_reclaimed_failure(
        "completed",
        Some("watchdog reclaimed stale running ingest job (idle=360s marker=amqp)")
    ));
    assert!(!is_watchdog_reclaimed_failure(
        "failed",
        Some("network timeout")
    ));
}

#[test]
fn status_filter_hides_reclaimed_by_default_and_shows_in_reclaimed_mode() {
    let reclaimed_err =
        Some("watchdog reclaimed stale running extract job (idle=360s marker=amqp)");
    assert!(!include_status_job("failed", reclaimed_err, false));
    assert!(include_status_job("failed", reclaimed_err, true));
    assert!(include_status_job("completed", None, false));
    assert!(!include_status_job("completed", None, true));
}

#[test]
fn status_view_mode_filters_active_and_recent() {
    assert!(include_status_view("running", true, false));
    assert!(!include_status_view("failed", true, false));
    assert!(include_status_view("completed", false, true));
    assert!(!include_status_view("canceled", false, true));
}
