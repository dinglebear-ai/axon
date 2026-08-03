use super::*;
use axon_api::source::{JobId, JobStagePlan, LifecycleStatus, PipelinePhase};
use uuid::Uuid;

use state_machine::{validate_stage_plan, validate_transition};

#[test]
fn job_state_machine_accepts_only_contract_transitions() {
    let job_id = JobId::new(Uuid::from_u128(1));
    let allowed = [
        (LifecycleStatus::Queued, LifecycleStatus::Blocked),
        (LifecycleStatus::Queued, LifecycleStatus::Running),
        (LifecycleStatus::Queued, LifecycleStatus::Failed),
        (LifecycleStatus::Queued, LifecycleStatus::Canceling),
        (LifecycleStatus::Queued, LifecycleStatus::Expired),
        (LifecycleStatus::Pending, LifecycleStatus::Queued),
        (LifecycleStatus::Pending, LifecycleStatus::Running),
        (LifecycleStatus::Pending, LifecycleStatus::Failed),
        (LifecycleStatus::Pending, LifecycleStatus::Canceling),
        (LifecycleStatus::Pending, LifecycleStatus::Expired),
        (LifecycleStatus::Blocked, LifecycleStatus::Queued),
        (LifecycleStatus::Blocked, LifecycleStatus::Running),
        (LifecycleStatus::Blocked, LifecycleStatus::Canceling),
        (LifecycleStatus::Blocked, LifecycleStatus::Failed),
        (LifecycleStatus::Blocked, LifecycleStatus::Expired),
        (LifecycleStatus::Running, LifecycleStatus::Waiting),
        (LifecycleStatus::Running, LifecycleStatus::Canceling),
        (LifecycleStatus::Running, LifecycleStatus::Completed),
        (LifecycleStatus::Running, LifecycleStatus::CompletedDegraded),
        (LifecycleStatus::Running, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Running),
        (LifecycleStatus::Waiting, LifecycleStatus::Canceling),
        (LifecycleStatus::Waiting, LifecycleStatus::Failed),
        (LifecycleStatus::Waiting, LifecycleStatus::Expired),
        (LifecycleStatus::Canceling, LifecycleStatus::Canceled),
        (LifecycleStatus::Canceling, LifecycleStatus::Failed),
    ];
    for (from, to) in allowed {
        validate_transition(job_id, from, to).expect("allowed transition");
    }

    for terminal in [
        LifecycleStatus::Completed,
        LifecycleStatus::CompletedDegraded,
        LifecycleStatus::Failed,
        LifecycleStatus::Canceled,
        LifecycleStatus::Expired,
        LifecycleStatus::Skipped,
    ] {
        let err = validate_transition(job_id, terminal, LifecycleStatus::Queued)
            .expect_err("terminal transition rejected");
        assert_eq!(err.code, "job.invalid_transition".into());
    }
}

#[test]
fn duplicate_stable_stage_keys_are_rejected() {
    let stages = vec![
        JobStagePlan::required(PipelinePhase::Fetching),
        JobStagePlan::required(PipelinePhase::Fetching),
    ];
    let err = validate_stage_plan(&stages).expect_err("duplicate stage key must fail");
    assert_eq!(err.code, "job.stage_plan.duplicate_key".into());

    let distinct = vec![
        JobStagePlan::required(PipelinePhase::Fetching).with_stage_key("fetch-primary"),
        JobStagePlan::required(PipelinePhase::Fetching).with_stage_key("fetch-fallback"),
    ];
    validate_stage_plan(&distinct).expect("explicit stable keys disambiguate repeated phases");
}
