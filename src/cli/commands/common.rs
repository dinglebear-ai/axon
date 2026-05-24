pub use super::common_jobs::{
    JobStatus, filter_jobs_for_status_view, handle_job_cancel, handle_job_cleanup,
    handle_job_clear, handle_job_errors, handle_job_list, handle_job_recover, handle_job_status,
    handle_worker_mode, include_job_for_status_view,
};
pub use super::common_urls::{parse_urls, start_url_from_cfg, truncate_chars};

use crate::core::ui::muted;
use crate::services::types::ServiceTimeRange;

pub const HUMAN_LINE_LIMIT: usize = 120;

/// Convert a CLI time-range string to the services-layer [`ServiceTimeRange`] enum.
///
/// Shared by `search` and `research` commands.
pub fn parse_service_time_range(value: Option<&str>) -> Option<ServiceTimeRange> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("day") => Some(ServiceTimeRange::Day),
        Some("week") => Some(ServiceTimeRange::Week),
        Some("month") => Some(ServiceTimeRange::Month),
        Some("year") => Some(ServiceTimeRange::Year),
        _ => None,
    }
}

pub fn truncate_display_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    format!("{}…", truncate_chars(text, max_chars - 1))
}

pub fn truncate_display_line(text: &str) -> String {
    truncate_display_text(text, HUMAN_LINE_LIMIT)
}

pub fn truncate_display_continuation(text: &str, indent_chars: usize) -> String {
    truncate_display_text(text, HUMAN_LINE_LIMIT.saturating_sub(indent_chars))
}

pub fn print_list_footer(shown: usize, total: i64, limit: i64, offset: i64) {
    if offset + limit < total {
        println!(
            "  {}",
            muted(&format!(
                "Showing {} of {} total — use --offset {} for next page",
                shown,
                total,
                offset + limit,
            ))
        );
    } else {
        println!("  {}", muted(&format!("{total} total")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_display_text_counts_ellipsis_inside_cap() {
        let value = truncate_display_text("abcdef", 4);

        assert_eq!(value, "abc…");
        assert_eq!(value.chars().count(), 4);
    }

    #[test]
    fn truncate_display_text_is_multibyte_safe() {
        let value = truncate_display_text("éééé", 3);

        assert_eq!(value, "éé…");
        assert_eq!(value.chars().count(), 3);
    }
}
