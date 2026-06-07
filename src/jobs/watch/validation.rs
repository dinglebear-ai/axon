use super::WatchDefCreate;

pub const WATCH_RUN_STATUS_RUNNING: &str = "running";
pub const WATCH_RUN_STATUS_COMPLETED: &str = "completed";
pub const WATCH_RUN_STATUS_FAILED: &str = "failed";

/// Task types a watch may carry. A `task_type` outside this set can never run,
/// so every create path (CLI, HTTP) must validate against this single list.
pub const SUPPORTED_TASK_TYPES: &[&str] = &["watch"];

/// Validate a `task_type` at create time so callers never persist a watch that
/// can never execute. Rejects surrounding whitespace (the stored value would
/// otherwise fail the exact-match dispatch) and any type outside
/// [`SUPPORTED_TASK_TYPES`]. The message is safe for entry points to surface.
pub fn validate_task_type(task_type: &str) -> Result<(), String> {
    if task_type != task_type.trim() {
        return Err("task_type must not have leading or trailing whitespace".to_string());
    }
    if !SUPPORTED_TASK_TYPES.contains(&task_type) {
        return Err(format!(
            "unsupported task_type: '{}'; supported: {}",
            task_type,
            SUPPORTED_TASK_TYPES.join(", ")
        ));
    }
    Ok(())
}

/// Maximum number of URLs a single watch may track. Bounds per-tick work and
/// the persisted payload.
pub const MAX_WATCH_URLS: usize = 256;
/// Maximum crawl depth a watch payload may request for change-triggered crawls.
pub const MAX_WATCH_DEPTH: u64 = 10;

/// Validate a watch's `task_payload` at create time: `urls` non-empty (and
/// within [`MAX_WATCH_URLS`]), every `ignore_patterns` entry compiles as a
/// regex, and any `max_depth` is within [`MAX_WATCH_DEPTH`]. Shared by CLI +
/// HTTP create.
pub fn validate_task_payload(payload: &serde_json::Value) -> Result<(), String> {
    let urls = payload
        .get("urls")
        .and_then(|v| v.as_array())
        .ok_or("task_payload.urls must be a non-empty array")?;
    if urls.is_empty() || !urls.iter().all(|u| u.is_string()) {
        return Err("task_payload.urls must be a non-empty array of strings".to_string());
    }
    if urls.len() > MAX_WATCH_URLS {
        return Err(format!(
            "task_payload.urls has {} entries; maximum is {MAX_WATCH_URLS}",
            urls.len()
        ));
    }
    if let Some(depth) = payload.get("max_depth") {
        let n = depth
            .as_u64()
            .ok_or("task_payload.max_depth must be a non-negative integer")?;
        if n > MAX_WATCH_DEPTH {
            return Err(format!(
                "task_payload.max_depth is {n}; maximum is {MAX_WATCH_DEPTH}"
            ));
        }
    }
    if let Some(pats) = payload.get("ignore_patterns") {
        let arr = pats
            .as_array()
            .ok_or("ignore_patterns must be an array of strings")?;
        for p in arr {
            let s = p
                .as_str()
                .ok_or("ignore_patterns entries must be strings")?;
            regex::Regex::new(s).map_err(|e| format!("invalid ignore_pattern '{s}': {e}"))?;
        }
    }
    Ok(())
}

/// Minimum allowed watch interval. The scheduler leases purely on
/// `next_run_at <= now`, so a sub-minimum interval would auto-fire too often.
pub const MIN_WATCH_INTERVAL_SECS: i64 = 30;
/// Maximum allowed watch interval (7 days).
pub const MAX_WATCH_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60;
pub(super) const DEFAULT_WATCH_LEASE_SECS: i64 = 300;
pub(super) const MAX_WATCH_LIST_LIMIT: i64 = 500;

/// Validate `every_seconds` at create time. Centralized (like
/// [`validate_task_type`]) so every create path — REST/admin `/v1/watch`,
/// and the CLI — enforces identical bounds and the
/// scheduler can never lease a sub-minimum watch. The message is safe to
/// surface to callers.
pub fn validate_every_seconds(every_seconds: i64) -> Result<(), String> {
    if !(MIN_WATCH_INTERVAL_SECS..=MAX_WATCH_INTERVAL_SECS).contains(&every_seconds) {
        return Err(format!(
            "every_seconds must be between {MIN_WATCH_INTERVAL_SECS} and {MAX_WATCH_INTERVAL_SECS}"
        ));
    }
    Ok(())
}

/// Validate all create-time watch invariants at the persistence boundary.
///
/// Entry points may still apply transport-specific checks (for example request
/// body size limits), but no caller should be able to persist a definition that
/// the scheduler cannot execute safely.
pub fn validate_watch_def_create(input: &WatchDefCreate) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    validate_task_type(&input.task_type)?;
    validate_every_seconds(input.every_seconds)?;
    validate_task_payload(&input.task_payload)?;
    if let Some(urls) = input.task_payload.get("urls").and_then(|v| v.as_array()) {
        for url_val in urls {
            let url = url_val
                .as_str()
                .ok_or("task_payload.urls entries must be strings")?;
            crate::core::http::validate_url(url)
                .map_err(|e| format!("invalid url in task_payload.urls: {e}"))?;
        }
    }
    Ok(())
}
