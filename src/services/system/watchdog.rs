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
#[path = "watchdog_tests.rs"]
mod tests;
