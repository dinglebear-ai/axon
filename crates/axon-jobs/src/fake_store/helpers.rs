use axon_api::source::*;
use uuid::Uuid;

use super::FakeJobWatchState;

pub(super) fn apply_visibility_filter(items: &mut Vec<JobEvent>, visibility: Option<Visibility>) {
    match visibility {
        Some(visibility) => items.retain(|event| event.visibility == visibility),
        None => items.retain(|event| {
            matches!(
                event.visibility,
                Visibility::Public | Visibility::Redacted | Visibility::Derived
            )
        }),
    }
}

pub(super) fn event_details(event: &SourceProgressEvent) -> MetadataMap {
    let mut details = MetadataMap::new();
    details.insert(
        "source_progress_event".to_string(),
        serde_json::to_value(event).unwrap_or(serde_json::Value::Null),
    );
    if let Some(dedupe_key) = &event.dedupe_key {
        details.insert("dedupe_key".to_string(), serde_json::json!(dedupe_key));
    }
    details
}

pub(super) fn append_event_locked(
    state: &mut FakeJobWatchState,
    event: SourceProgressEvent,
) -> crate::boundary::Result<()> {
    if !state.jobs.contains_key(&event.job_id) {
        return Err(missing_job(event.job_id));
    }
    let expected_sequence = state
        .events
        .get(&event.job_id)
        .and_then(|events| events.last())
        .map(|event| event.sequence + 1)
        .unwrap_or(1);
    if event.sequence != expected_sequence {
        if let Some(dedupe_key) = event.dedupe_key.as_ref()
            && has_dedupe_key_at_sequence(state, event.job_id, dedupe_key, event.sequence)
        {
            return Ok(());
        }
        return Err(ApiError::new(
            "job_event.sequence_invalid",
            ErrorStage::Publishing,
            format!(
                "expected event sequence {} for job {}, got {}",
                expected_sequence, event.job_id.0, event.sequence
            ),
        ));
    }
    let duplicate_dedupe = event
        .dedupe_key
        .as_ref()
        .is_some_and(|dedupe_key| has_dedupe_key(state, event.job_id, dedupe_key));
    let mut details = event_details(&event);
    if duplicate_dedupe {
        details.remove("dedupe_key");
        details.insert("dedupe_duplicate".to_string(), serde_json::json!(true));
    }
    state
        .events
        .entry(event.job_id)
        .or_default()
        .push(JobEvent {
            event_id: event.event_id,
            sequence: event.sequence,
            job_id: event.job_id,
            attempt: event.attempt,
            stage_id: event.stage_id,
            phase: event.phase,
            status: event.status,
            severity: event.severity,
            visibility: event.visibility,
            message: event.message,
            timestamp: event.timestamp,
            details,
        });
    Ok(())
}

pub(super) fn terminal_cleanup_eligible(
    job: &JobSummary,
    now: &Timestamp,
    older_than_seconds: Option<u64>,
) -> bool {
    if !is_terminal_status(job.status) {
        return false;
    }
    let Some(seconds) = older_than_seconds else {
        return true;
    };
    timestamp_age_seconds(now, &job.updated_at).is_some_and(|age| age >= seconds)
}

pub(super) fn retry_locked(
    state: &mut FakeJobWatchState,
    job_id: JobId,
    request: JobRetryRequest,
) -> crate::boundary::Result<JobRetryResult> {
    if request.mode == JobRetryMode::SameConfig && !request.overrides.is_empty() {
        return Err(ApiError::new(
            "job_retry.overrides_forbidden",
            ErrorStage::Planning,
            "same_config retry cannot include overrides",
        ));
    }
    let updated_at = state.timestamp();
    let original = state
        .jobs
        .get(&job_id)
        .cloned()
        .ok_or_else(|| missing_job(job_id))?;
    if matches!(
        original.status,
        LifecycleStatus::Running | LifecycleStatus::Waiting | LifecycleStatus::Canceling
    ) {
        return Err(ApiError::new(
            "job_retry.active_job",
            ErrorStage::Planning,
            "only terminal, blocked, queued, or pending jobs can be retried",
        ));
    }
    let attempt = original.attempt + 1;
    let mut stage_plan = state
        .stages
        .get(&job_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|stage| JobStagePlan {
            phase: stage.phase,
            required: stage.required,
            provider_requirements: stage.provider_requirements,
            estimated_items: stage.counts.items_total,
        })
        .collect::<Vec<_>>();
    if let Some(from_phase) = request.from_phase {
        let Some(index) = stage_plan
            .iter()
            .position(|stage| stage.phase == from_phase)
        else {
            return Err(ApiError::new(
                "job_retry.from_phase_not_found",
                ErrorStage::Planning,
                format!("phase {:?} is not present in job {}", from_phase, job_id.0),
            ));
        };
        stage_plan = stage_plan.split_off(index);
    }
    if let Some(job) = state.jobs.get_mut(&job_id) {
        job.intent = Some(JobIntent::Retry);
        job.status = LifecycleStatus::Queued;
        job.phase = PipelinePhase::Queued;
        job.attempt = attempt;
        job.started_at = None;
        job.finished_at = None;
        job.last_error = None;
        job.updated_at = updated_at;
    }
    let mut stages = Vec::new();
    for stage in stage_plan {
        state.next_stage += 1;
        stages.push(JobStageSnapshot {
            stage_id: StageId::new(Uuid::from_u128(state.next_stage)),
            phase: stage.phase,
            status: LifecycleStatus::Queued,
            required: stage.required,
            provider_requirements: stage.provider_requirements,
            counts: empty_counts(),
            started_at: None,
            completed_at: None,
            error: None,
        });
    }
    state.stages.insert(job_id, stages);
    if let Some(key) = request.idempotency_key {
        state.idempotency_keys.insert(key, job_id);
    }
    let retry_job = state
        .jobs
        .get(&job_id)
        .map(descriptor)
        .ok_or_else(|| missing_job(job_id))?;
    Ok(JobRetryResult {
        original_job_id: job_id,
        retry_job,
        attempt,
    })
}

pub(super) fn new_job_descriptor(
    job_id: JobId,
    kind: JobKind,
    timestamp: Timestamp,
) -> JobDescriptor {
    JobDescriptor {
        job_id,
        kind,
        status: LifecycleStatus::Queued,
        poll: PollDescriptor {
            status_url: format!("/v1/jobs/{job_id}", job_id = job_id.0),
            events_url: Some(format!("/v1/jobs/{job_id}/events", job_id = job_id.0)),
            suggested_interval_ms: 1000,
        },
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }
}

pub(super) fn descriptor(summary: &JobSummary) -> JobDescriptor {
    JobDescriptor {
        job_id: summary.job_id,
        kind: summary.kind,
        status: summary.status,
        poll: PollDescriptor {
            status_url: format!("/v1/jobs/{job_id}", job_id = summary.job_id.0),
            events_url: Some(format!(
                "/v1/jobs/{job_id}/events",
                job_id = summary.job_id.0
            )),
            suggested_interval_ms: 1000,
        },
        created_at: summary.created_at.clone(),
        updated_at: summary.updated_at.clone(),
    }
}

pub(super) fn empty_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}

pub(super) fn is_stale(job: &JobSummary, now: &Timestamp, older_than_seconds: Option<u64>) -> bool {
    let Some(seconds) = older_than_seconds else {
        return true;
    };
    let reference = job
        .heartbeat
        .as_ref()
        .map(|heartbeat| &heartbeat.heartbeat_at)
        .unwrap_or(&job.updated_at);
    timestamp_age_seconds(now, reference).is_some_and(|age| age >= seconds)
}

pub(super) fn capability(name: &str) -> CapabilityBase {
    CapabilityBase {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        owner_crate: "axon-jobs".to_string(),
        health: HealthStatus::Healthy,
        features: vec!["fake".to_string()],
        limits: MetadataMap::new(),
    }
}

pub(super) fn is_terminal_status(status: LifecycleStatus) -> bool {
    matches!(
        status,
        LifecycleStatus::Completed
            | LifecycleStatus::CompletedDegraded
            | LifecycleStatus::Failed
            | LifecycleStatus::Canceled
            | LifecycleStatus::Expired
            | LifecycleStatus::Skipped
    )
}

pub(super) fn source_error_to_api_error(error: &SourceError) -> ApiError {
    ApiError::new(
        error.code.clone(),
        ErrorStage::Publishing,
        error.message.clone(),
    )
}

pub(super) fn recovery_api_error() -> ApiError {
    ApiError::new(
        "job.recovered_stale",
        ErrorStage::Publishing,
        "stale running job was failed by recovery",
    )
}

impl FakeJobWatchState {
    pub(super) fn timestamp(&mut self) -> Timestamp {
        self.next_tick += 1;
        Timestamp(format!("2026-07-01T00:00:{:02}Z", self.next_tick))
    }

    pub(super) fn peek_timestamp(&self) -> Timestamp {
        Timestamp(format!("2026-07-01T00:00:{:02}Z", self.next_tick + 1))
    }
}

pub(super) fn missing_job(job_id: JobId) -> ApiError {
    ApiError::new(
        "job.not_found",
        ErrorStage::Retrieving,
        format!("job {} not found", job_id.0),
    )
}

pub(super) fn missing_stage(job_id: JobId, stage_id: StageId) -> ApiError {
    ApiError::new(
        "job_stage.not_found",
        ErrorStage::Publishing,
        format!("stage {} not found for job {}", stage_id.0, job_id.0),
    )
}

pub(super) fn missing_watch(watch_id: &WatchId) -> ApiError {
    ApiError::new(
        "watch.not_found",
        ErrorStage::Retrieving,
        format!("watch {} not found", watch_id.0),
    )
}

fn timestamp_age_seconds(now: &Timestamp, then: &Timestamp) -> Option<u64> {
    let now = chrono::DateTime::parse_from_rfc3339(&now.0).ok()?;
    let then = chrono::DateTime::parse_from_rfc3339(&then.0).ok()?;
    (now - then).num_seconds().try_into().ok()
}

fn has_dedupe_key(state: &FakeJobWatchState, job_id: JobId, dedupe_key: &str) -> bool {
    state
        .events
        .get(&job_id)
        .into_iter()
        .flatten()
        .any(|existing| existing.details.get("dedupe_key") == Some(&serde_json::json!(dedupe_key)))
}

fn has_dedupe_key_at_sequence(
    state: &FakeJobWatchState,
    job_id: JobId,
    dedupe_key: &str,
    sequence: u64,
) -> bool {
    state
        .events
        .get(&job_id)
        .into_iter()
        .flatten()
        .any(|existing| {
            existing.sequence == sequence
                && existing.details.get("dedupe_key") == Some(&serde_json::json!(dedupe_key))
        })
}
