use axon_api::source::*;
use uuid::Uuid;

use super::FakeJobWatchStore;
use super::helpers::*;
use crate::boundary::Result;
use crate::state_machine::validate_stage_plan;

pub(super) async fn create(
    store: &FakeJobWatchStore,
    request: JobCreateRequest,
) -> Result<JobDescriptor> {
    validate_stage_plan(&request.stage_plan)?;
    let mut state = store.state.lock().await;
    if let Some(job_id) = request
        .idempotency_key
        .as_ref()
        .and_then(|key| state.idempotency_keys.get(key).copied())
    {
        let summary = state
            .jobs
            .get(&job_id)
            .cloned()
            .ok_or_else(|| missing_job(job_id))?;
        return Ok(descriptor(&summary));
    }
    state.next_job += 1;
    let job_id = JobId::new(Uuid::from_u128(state.next_job));
    let root_job_id = request.root_job_id.unwrap_or(job_id);
    let created_at = state.timestamp();
    let summary = JobSummary {
        job_id,
        kind: request.job_kind,
        intent: Some(request.job_intent),
        status: LifecycleStatus::Queued,
        phase: PipelinePhase::Queued,
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
        started_at: None,
        finished_at: None,
        source_id: request.source_id.clone(),
        watch_id: request.watch_id.clone(),
        parent_job_id: request.parent_job_id,
        root_job_id: Some(root_job_id),
        attempt: 0,
        priority: request.priority,
        counts: None,
        current: None,
        heartbeat: None,
        last_error: None,
        warnings: Vec::new(),
    };
    state.jobs.insert(job_id, summary);
    if let Some(request_json) = request.request.clone() {
        state.requests.insert(job_id, request_json);
    }
    let stages = request
        .stage_plan
        .into_iter()
        .enumerate()
        .map(|(ordinal, stage)| JobStageSnapshot {
            stage_id: stage.stable_id(job_id, ordinal),
            phase: stage.phase,
            status: LifecycleStatus::Queued,
            required: stage.required,
            provider_requirements: stage.provider_requirements,
            counts: empty_counts(),
            started_at: None,
            completed_at: None,
            error: None,
        })
        .collect();
    state.stages.insert(job_id, stages);
    if let Some(key) = request.idempotency_key {
        state.idempotency_keys.insert(key, job_id);
    }
    Ok(new_job_descriptor(job_id, request.job_kind, created_at))
}
