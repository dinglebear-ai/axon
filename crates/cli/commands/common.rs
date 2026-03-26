pub use super::common_jobs::{
    JobStatus, filter_jobs_for_status_view, handle_job_cancel, handle_job_cleanup,
    handle_job_clear, handle_job_errors, handle_job_list, handle_job_recover, handle_job_status,
    handle_worker_mode, include_job_for_status_view,
};
pub use super::common_urls::{parse_urls, start_url_from_cfg, truncate_chars};

use crate::crates::services::types::ServiceTimeRange;

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
