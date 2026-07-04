use axon_api::source::{ApiError, ErrorStage, JobId, LifecycleStatus};

pub(crate) fn validate_transition(
    job_id: JobId,
    from: LifecycleStatus,
    to: LifecycleStatus,
) -> Result<(), ApiError> {
    if from == to {
        return Ok(());
    }

    let allowed = matches!(
        (from, to),
        (LifecycleStatus::Queued, LifecycleStatus::Blocked)
            | (LifecycleStatus::Queued, LifecycleStatus::Running)
            | (LifecycleStatus::Queued, LifecycleStatus::Canceling)
            | (LifecycleStatus::Queued, LifecycleStatus::Expired)
            | (LifecycleStatus::Pending, LifecycleStatus::Queued)
            | (LifecycleStatus::Pending, LifecycleStatus::Running)
            | (LifecycleStatus::Pending, LifecycleStatus::Canceling)
            | (LifecycleStatus::Pending, LifecycleStatus::Expired)
            | (LifecycleStatus::Blocked, LifecycleStatus::Queued)
            | (LifecycleStatus::Blocked, LifecycleStatus::Running)
            | (LifecycleStatus::Blocked, LifecycleStatus::Canceling)
            | (LifecycleStatus::Blocked, LifecycleStatus::Failed)
            | (LifecycleStatus::Blocked, LifecycleStatus::Expired)
            | (LifecycleStatus::Running, LifecycleStatus::Waiting)
            | (LifecycleStatus::Running, LifecycleStatus::Canceling)
            | (LifecycleStatus::Running, LifecycleStatus::Completed)
            | (LifecycleStatus::Running, LifecycleStatus::CompletedDegraded)
            | (LifecycleStatus::Running, LifecycleStatus::Failed)
            | (LifecycleStatus::Waiting, LifecycleStatus::Running)
            | (LifecycleStatus::Waiting, LifecycleStatus::Canceling)
            | (LifecycleStatus::Waiting, LifecycleStatus::Failed)
            | (LifecycleStatus::Waiting, LifecycleStatus::Expired)
            | (LifecycleStatus::Canceling, LifecycleStatus::Canceled)
            | (LifecycleStatus::Canceling, LifecycleStatus::Failed)
    );

    if allowed {
        return Ok(());
    }

    Err(ApiError::new(
        "job.invalid_transition",
        ErrorStage::Publishing,
        format!(
            "cannot transition job {} from {:?} to {:?}",
            job_id.0, from, to
        ),
    ))
}
