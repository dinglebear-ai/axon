use std::collections::BTreeSet;

use axon_api::source::{ApiError, ErrorStage, JobId, JobStagePlan, LifecycleStatus};

pub(crate) fn validate_stage_plan(stage_plan: &[JobStagePlan]) -> Result<(), ApiError> {
    let mut keys = BTreeSet::new();
    for stage in stage_plan {
        let key = stage.effective_stage_key();
        if !keys.insert(key.to_string()) {
            return Err(ApiError::new(
                "job.stage_plan.duplicate_key",
                ErrorStage::Planning,
                format!("stage plan contains duplicate stable key `{key}`"),
            )
            .with_context("stage_key", key));
        }
    }
    Ok(())
}

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
            | (LifecycleStatus::Queued, LifecycleStatus::Failed)
            | (LifecycleStatus::Queued, LifecycleStatus::Canceling)
            | (LifecycleStatus::Queued, LifecycleStatus::Expired)
            | (LifecycleStatus::Pending, LifecycleStatus::Queued)
            | (LifecycleStatus::Pending, LifecycleStatus::Running)
            | (LifecycleStatus::Pending, LifecycleStatus::Failed)
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
            // Deadline enforcement (R1-V01): a running attempt that passed
            // its `deadline_at` is expired by the watchdog/claim path. Not
            // in the base contract's transition table (which only reaches
            // `expired` from queued/pending/blocked/waiting), but additive
            // per this implementation's deadline feature.
            | (LifecycleStatus::Running, LifecycleStatus::Expired)
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
