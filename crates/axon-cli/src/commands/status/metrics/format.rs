use chrono::{DateTime, Utc};

pub(super) fn format_duration(mut secs: u64) -> String {
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3_600;
    secs %= 3_600;
    let minutes = secs / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// Human-readable relative age: "3s ago", "12m ago", "2h ago", "4d ago".
pub fn format_age(ts: &DateTime<Utc>) -> String {
    let secs = (Utc::now() - *ts).num_seconds().max(0) as u64;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Human-readable run duration.
///
/// Terminal jobs show total runtime from `started_at` to `finished_at`.
/// Active jobs show elapsed runtime from `started_at` to now.
/// If runtime anchors are missing, falls back to relative age.
pub(crate) fn job_runtime_text(
    status: &str,
    started_at: Option<&DateTime<Utc>>,
    finished_at: Option<&DateTime<Utc>>,
    updated_at: &DateTime<Utc>,
) -> String {
    match status {
        "completed" | "failed" | "canceled" => {
            if let (Some(started), Some(finished)) = (started_at, finished_at) {
                let secs = (*finished - *started).num_seconds().max(0) as u64;
                format_duration(secs)
            } else {
                format_age(finished_at.unwrap_or(updated_at))
            }
        }
        "running" | "processing" | "scraping" => {
            if let Some(started) = started_at {
                let secs = (Utc::now() - *started).num_seconds().max(0) as u64;
                format_duration(secs)
            } else {
                format_age(updated_at)
            }
        }
        _ => format_age(updated_at),
    }
}

/// First line of error_text, truncated to 60 chars.
pub(crate) fn format_error(error_text: Option<&str>) -> Option<String> {
    let text = error_text?.trim();
    if text.is_empty() {
        return None;
    }
    let first_line = text.lines().next().unwrap_or(text);
    if first_line.chars().count() > 60 {
        Some(format!(
            "{}…",
            crate::commands::common::truncate_chars(first_line, 60)
        ))
    } else {
        Some(first_line.to_string())
    }
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
