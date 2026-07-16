//! Neutral watch schedule validation shared by source-watch surfaces.
//!
//! This module deliberately contains no legacy task-payload/watch-def types.

/// Minimum allowed watch interval. The scheduler leases purely on
/// `next_run_at <= now`, so a sub-minimum interval would auto-fire too often.
pub const MIN_WATCH_INTERVAL_SECS: i64 = 30;
/// Maximum allowed watch interval (7 days).
pub const MAX_WATCH_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60;
pub(crate) const DEFAULT_WATCH_LEASE_SECS: i64 = 300;

/// Validate `every_seconds` at create time. The message is safe to surface to
/// CLI, REST, and MCP callers.
pub fn validate_every_seconds(every_seconds: i64) -> Result<(), String> {
    if !(MIN_WATCH_INTERVAL_SECS..=MAX_WATCH_INTERVAL_SECS).contains(&every_seconds) {
        return Err(format!(
            "every_seconds must be between {MIN_WATCH_INTERVAL_SECS} and {MAX_WATCH_INTERVAL_SECS}"
        ));
    }
    Ok(())
}

pub(crate) fn parse_watch_lease_secs(raw: Option<String>) -> i64 {
    raw.and_then(|raw| raw.parse::<i64>().ok())
        .filter(|secs| *secs >= 1)
        .unwrap_or(DEFAULT_WATCH_LEASE_SECS)
}
