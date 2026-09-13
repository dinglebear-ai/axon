use axon_api::source::{ApiError, JobId, LifecycleStatus};
use axon_core::logging::log_warn;
use axon_jobs::boundary::JobStore;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::unified::UnifiedClaimedJob;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttemptOwnership {
    Active,
    Lost,
    Unknown,
}

pub(super) fn classify_attempt_ownership(
    result: Result<Option<(u32, LifecycleStatus)>, ApiError>,
    claimed_attempt: u32,
    job_id: JobId,
) -> AttemptOwnership {
    match result {
        Ok(Some((attempt, status)))
            if attempt == claimed_attempt
                && matches!(status, LifecycleStatus::Running | LifecycleStatus::Waiting) =>
        {
            AttemptOwnership::Active
        }
        Ok(_) => AttemptOwnership::Lost,
        Err(error) => {
            log_warn(&format!(
                "attempt ownership read failed for job {}: {error}",
                job_id.0
            ));
            AttemptOwnership::Unknown
        }
    }
}

pub(super) async fn attempt_ownership(
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
) -> AttemptOwnership {
    let result = store
        .get(claimed.job_id)
        .await
        .map(|summary| summary.map(|summary| (summary.attempt, summary.status)));
    classify_attempt_ownership(result, claimed.attempt, claimed.job_id)
}
