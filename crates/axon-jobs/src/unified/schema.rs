#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const CONTRACT_INDEXES: &[&str] = &[
    "idx_axon_jobs_status_kind_updated",
    "idx_axon_jobs_source_status_updated",
    "idx_axon_jobs_watch_status_updated",
    "idx_axon_job_events_job_sequence",
    "idx_axon_job_events_job_severity_sequence",
    "idx_axon_job_attempts_job_attempt",
    "idx_axon_job_stages_job_stage",
];
