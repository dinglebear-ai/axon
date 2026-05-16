//! Watchdog reclaim helpers — detect / filter jobs reclaimed from stale runs.
//!
//! Note: the error_text written by the SQLite watchdog itself
//! (`reclaimed after unexpected shutdown`) lives in
//! `src/jobs/lite/store.rs::RECLAIMED_ERROR_TEXT`. The prefix below is a
//! legacy/alternative marker used by historical workers and is retained here
//! purely as a status-filter input.

pub(crate) const WATCHDOG_RECLAIM_PREFIX: &str = "watchdog reclaimed stale running ";

pub(crate) fn is_watchdog_reclaimed_failure(status: &str, error_text: Option<&str>) -> bool {
    if status != "failed" {
        return false;
    }
    error_text
        .map(str::trim_start)
        .is_some_and(|text| text.starts_with(WATCHDOG_RECLAIM_PREFIX))
}

pub(crate) fn include_status_job(
    status: &str,
    error_text: Option<&str>,
    reclaimed_only: bool,
) -> bool {
    let reclaimed = is_watchdog_reclaimed_failure(status, error_text);
    if reclaimed_only {
        reclaimed
    } else {
        !reclaimed
    }
}

pub(crate) fn include_status_view(status: &str, active_only: bool, recent_only: bool) -> bool {
    if active_only {
        return matches!(status, "pending" | "running" | "processing" | "scraping");
    }
    if recent_only {
        return matches!(
            status,
            "pending" | "running" | "processing" | "scraping" | "completed"
        );
    }
    true
}

#[cfg(test)]
mod tests {
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
}
