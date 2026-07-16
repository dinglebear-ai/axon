use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::boundary::{JobDeleteResult, JobStore, Result};
use crate::limits::clamp_page_limit;
use crate::state_machine::validate_transition;
use crate::unified::pagination::{
    EventCursor, JobCursor, decode_event_cursor, decode_job_cursor, encode_event_cursor,
    encode_job_cursor,
};
use crate::unified_codec::reject_non_public_visibility;

#[path = "fake_store/helpers.rs"]
mod helpers;
#[path = "fake_store/watch.rs"]
mod watch;

use helpers::*;

#[derive(Debug, Clone, Default)]
pub struct FakeJobWatchStore {
    state: Arc<Mutex<FakeJobWatchState>>,
}

#[derive(Debug, Default)]
struct FakeJobWatchState {
    jobs: BTreeMap<JobId, JobSummary>,
    requests: BTreeMap<JobId, serde_json::Value>,
    stages: BTreeMap<JobId, Vec<JobStageSnapshot>>,
    events: BTreeMap<JobId, Vec<JobEvent>>,
    idempotency_keys: BTreeMap<String, JobId>,
    watches: BTreeMap<WatchId, WatchResult>,
    watch_runs: BTreeMap<WatchId, Vec<JobId>>,
    next_job: u128,
    next_stage: u128,
    next_watch: u64,
    next_tick: u64,
}

impl FakeJobWatchStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl JobStore for FakeJobWatchStore {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor> {
        let mut state = self.state.lock().await;
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
        let mut stages = Vec::new();
        for stage in request.stage_plan {
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
        Ok(new_job_descriptor(job_id, request.job_kind, created_at))
    }

    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        Ok(self.state.lock().await.jobs.get(&job_id).cloned())
    }

    async fn request_json(&self, job_id: JobId) -> Result<Option<serde_json::Value>> {
        Ok(self.state.lock().await.requests.get(&job_id).cloned())
    }

    async fn attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>> {
        let Some(summary) = self.state.lock().await.jobs.get(&job_id).cloned() else {
            return Err(missing_job(job_id));
        };
        Ok(summary
            .heartbeat
            .map(|heartbeat| JobAttemptSnapshot {
                attempt: heartbeat.attempt,
                status: heartbeat.status,
                worker_id: heartbeat.worker_id,
                started_at: summary.started_at.unwrap_or(summary.created_at),
                finished_at: summary.finished_at,
                heartbeat_at: Some(heartbeat.heartbeat_at),
                error: None,
            })
            .into_iter()
            .collect())
    }

    async fn stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>> {
        let state = self.state.lock().await;
        if !state.jobs.contains_key(&job_id) {
            return Err(missing_job(job_id));
        }
        Ok(state.stages.get(&job_id).cloned().unwrap_or_default())
    }

    async fn update_status(&self, status: JobStatusUpdate) -> Result<()> {
        let mut state = self.state.lock().await;
        let updated_at = state.timestamp();
        let stage_counts = status.counts.clone();
        {
            let job = state
                .jobs
                .get_mut(&status.job_id)
                .ok_or_else(|| missing_job(status.job_id))?;
            validate_transition(status.job_id, job.status, status.status)?;
            if let Some(source_id) = status.source_id.clone() {
                job.source_id = Some(source_id);
            }
            job.status = status.status;
            job.phase = status.phase;
            job.counts = status.counts;
            job.current = status.current;
            job.last_error = status.error.clone();
            job.updated_at = updated_at.clone();
            if status.status == LifecycleStatus::Running && job.started_at.is_none() {
                job.started_at = Some(updated_at.clone());
            }
            if is_terminal_status(status.status) {
                job.finished_at = Some(updated_at.clone());
            }
        }
        if let Some(stage_id) = status.stage_id {
            let Some(stages) = state.stages.get_mut(&status.job_id) else {
                return Err(missing_stage(status.job_id, stage_id));
            };
            let Some(stage) = stages.iter_mut().find(|stage| stage.stage_id == stage_id) else {
                return Err(missing_stage(status.job_id, stage_id));
            };
            stage.status = status.status;
            if let Some(counts) = stage_counts {
                stage.counts = counts;
            }
            if status.status == LifecycleStatus::Running && stage.started_at.is_none() {
                stage.started_at = Some(updated_at.clone());
            }
            if is_terminal_status(status.status) {
                stage.completed_at = Some(updated_at);
            }
            stage.error = status.error.as_ref().map(source_error_to_api_error);
        }
        Ok(())
    }

    async fn append_event(&self, event: SourceProgressEvent) -> Result<()> {
        let mut state = self.state.lock().await;
        append_event_locked(&mut state, event)
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .get_mut(&heartbeat.job_id)
            .ok_or_else(|| missing_job(heartbeat.job_id))?;
        if job.status != heartbeat.status {
            validate_transition(heartbeat.job_id, job.status, heartbeat.status)?;
        }
        job.phase = heartbeat.phase;
        job.status = heartbeat.status;
        job.updated_at = heartbeat.heartbeat_at.clone();
        job.counts = heartbeat.counts.clone();
        job.heartbeat = Some(heartbeat);
        Ok(())
    }

    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_job_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("job.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        let mut items = self
            .state
            .lock()
            .await
            .jobs
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if let Some(status) = request.status {
            items.retain(|job| job.status == status);
        }
        if let Some(kind) = request.kind {
            items.retain(|job| job.kind == kind);
        }
        if let Some(source_id) = request.source_id {
            items.retain(|job| job.source_id.as_ref() == Some(&source_id));
        }
        if let Some(watch_id) = request.watch_id {
            items.retain(|job| job.watch_id.as_ref() == Some(&watch_id));
        }
        items.sort_by(|left, right| {
            right
                .updated_at
                .0
                .cmp(&left.updated_at.0)
                .then_with(|| right.job_id.0.cmp(&left.job_id.0))
        });
        if let Some(cursor) = cursor.as_ref() {
            items.retain(|job| {
                job.updated_at.0 < cursor.updated_at
                    || (job.updated_at.0 == cursor.updated_at
                        && job.job_id.0.to_string() < cursor.job_id)
            });
        }
        let total = cursor.is_none().then_some(items.len() as u64);
        let limit = clamp_page_limit(request.limit);
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = items.last().filter(|_| has_more).map(|job| {
            encode_job_cursor(&JobCursor {
                updated_at: job.updated_at.0.clone(),
                job_id: job.job_id.0.to_string(),
            })
        });
        Ok(Page {
            total,
            limit,
            next_cursor,
            items,
        })
    }

    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage> {
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_event_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("job_event.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        reject_non_public_visibility(request.visibility)?;
        let mut items = self
            .state
            .lock()
            .await
            .events
            .get(&request.job_id)
            .cloned()
            .unwrap_or_default();
        if let Some(phase) = request.phase {
            items.retain(|event| event.phase == phase);
        }
        if let Some(severity) = request.severity {
            items.retain(|event| event.severity == severity);
        }
        apply_visibility_filter(&mut items, request.visibility);
        if let Some(after_sequence) = cursor
            .map(|cursor| cursor.sequence)
            .or(request.after_sequence)
            .or(request.since_sequence)
        {
            items.retain(|event| event.sequence > after_sequence);
        }
        let limit = clamp_page_limit(request.limit);
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = items.last().filter(|_| has_more).map(|event| {
            encode_event_cursor(&EventCursor {
                sequence: event.sequence,
            })
        });
        Ok(JobEventPage {
            last_sequence: items.last().map(|event| event.sequence).unwrap_or(0),
            limit,
            next_cursor,
            events: items,
        })
    }

    async fn latest_event_sequence(&self, job_id: JobId) -> Result<Option<u64>> {
        if !self.state.lock().await.jobs.contains_key(&job_id) {
            return Err(missing_job(job_id));
        }
        Ok(self
            .state
            .lock()
            .await
            .events
            .get(&job_id)
            .and_then(|events| events.last())
            .map(|event| event.sequence))
    }

    async fn cancel(&self, job_id: JobId, request: JobCancelRequest) -> Result<JobCancelResult> {
        let mut state = self.state.lock().await;
        let updated_at = state.timestamp();
        let job = state
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| missing_job(job_id))?;
        validate_transition(job_id, job.status, LifecycleStatus::Canceling)?;
        let last_safe_stage = job.phase;
        let target = if matches!(
            job.status,
            LifecycleStatus::Queued | LifecycleStatus::Pending
        ) || request.force_after_ms == Some(0)
        {
            LifecycleStatus::Canceled
        } else {
            LifecycleStatus::Canceling
        };
        job.status = target;
        job.phase = PipelinePhase::Canceled;
        job.updated_at = updated_at.clone();
        if target == LifecycleStatus::Canceled {
            job.finished_at = Some(updated_at.clone());
        }
        if target == LifecycleStatus::Canceled
            && let Some(stages) = state.stages.get_mut(&job_id)
        {
            for stage in stages {
                if !is_terminal_status(stage.status) {
                    stage.status = LifecycleStatus::Canceled;
                    stage.completed_at = Some(updated_at.clone());
                }
            }
        }
        Ok(JobCancelResult {
            job_id,
            status: target,
            canceled_at: (target == LifecycleStatus::Canceled).then_some(updated_at),
            reason: request.reason,
            canceled_by: request.actor,
            last_safe_stage: Some(last_safe_stage),
            side_effects: Vec::new(),
            cleanup_debt_ids: Vec::new(),
        })
    }

    async fn retry(&self, job_id: JobId, request: JobRetryRequest) -> Result<JobRetryResult> {
        let mut state = self.state.lock().await;
        retry_locked(&mut state, job_id, request)
    }

    async fn recover(&self, request: JobRecoveryRequest) -> Result<JobRecoveryResult> {
        if request.older_than_seconds.is_none() && !request.allow_without_cutoff {
            return Err(ApiError::new(
                "job_recovery.cutoff_required",
                ErrorStage::Planning,
                "recovery requires older_than_seconds unless allow_without_cutoff is explicit",
            ));
        }
        let mut state = self.state.lock().await;
        let now = state.timestamp();
        let mut scanned = 0;
        let mut failed = 0;
        for job in state.jobs.values_mut() {
            if request.kind.is_some_and(|kind| kind != job.kind)
                || !matches!(
                    job.status,
                    LifecycleStatus::Running | LifecycleStatus::Waiting
                )
                || !is_stale(job, &now, request.older_than_seconds)
            {
                continue;
            }
            scanned += 1;
            if !request.dry_run {
                job.status = LifecycleStatus::Failed;
                job.phase = PipelinePhase::Complete;
                job.updated_at = now.clone();
                job.finished_at = Some(now.clone());
                if let Some(heartbeat) = job.heartbeat.as_mut() {
                    heartbeat.status = LifecycleStatus::Failed;
                    heartbeat.phase = PipelinePhase::Complete;
                    heartbeat.heartbeat_at = now.clone();
                }
                failed += 1;
            }
        }
        if !request.dry_run {
            for job_id in state
                .jobs
                .iter()
                .filter_map(|(job_id, job)| {
                    (job.status == LifecycleStatus::Failed && job.updated_at == now)
                        .then_some(*job_id)
                })
                .collect::<Vec<_>>()
            {
                if let Some(stages) = state.stages.get_mut(&job_id) {
                    for stage in stages {
                        if matches!(
                            stage.status,
                            LifecycleStatus::Running | LifecycleStatus::Waiting
                        ) {
                            stage.status = LifecycleStatus::Failed;
                            stage.completed_at = Some(now.clone());
                            stage.error = Some(recovery_api_error());
                        }
                    }
                }
            }
        }
        Ok(JobRecoveryResult {
            recovered: 0,
            job_ids: Vec::new(),
            warnings: Vec::new(),
            jobs_scanned: scanned,
            jobs_requeued: 0,
            jobs_failed: failed,
        })
    }

    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult> {
        if request.older_than_seconds.is_none() && !request.confirm_all_terminal {
            return Err(ApiError::new(
                "job_cleanup.cutoff_required",
                ErrorStage::Planning,
                "cleanup requires older_than_seconds unless confirm_all_terminal is explicit",
            ));
        }
        let mut state = self.state.lock().await;
        let now = state.timestamp();
        let terminal_ids = state
            .jobs
            .iter()
            .filter_map(|(job_id, job)| {
                terminal_cleanup_eligible(job, &now, request.older_than_seconds).then_some(*job_id)
            })
            .collect::<Vec<_>>();
        let events_pruned = terminal_ids
            .iter()
            .filter_map(|job_id| state.events.get(job_id).map(Vec::len))
            .sum::<usize>() as u64;
        if !request.dry_run {
            for job_id in &terminal_ids {
                state.jobs.remove(job_id);
                state.events.remove(job_id);
            }
        }
        Ok(JobCleanupResult {
            matched: terminal_ids.len() as u64,
            deleted: terminal_ids.len() as u64,
            dry_run: request.dry_run,
            warnings: Vec::new(),
            jobs_pruned: terminal_ids.len() as u64,
            events_pruned,
            heartbeats_pruned: 0,
            attempts_pruned: 0,
            stages_pruned: 0,
            reservations_pruned: 0,
            artifacts_pruned: 0,
        })
    }

    async fn delete_jobs(&self, job_ids: &[JobId]) -> Result<JobDeleteResult> {
        let mut state = self.state.lock().await;
        let mut result = JobDeleteResult::default();
        for job_id in job_ids {
            match state.jobs.get(job_id) {
                None => result.missing.push(*job_id),
                Some(job) if is_terminal_status(job.status) => {
                    result.deleted.push(*job_id);
                }
                Some(_) => result.skipped_live.push(*job_id),
            }
        }
        for job_id in &result.deleted {
            state.jobs.remove(job_id);
            state.requests.remove(job_id);
            state.stages.remove(job_id);
            state.events.remove(job_id);
        }
        Ok(result)
    }

    async fn artifacts(&self, request: JobArtifactListRequest) -> Result<JobArtifactListResult> {
        if !self.state.lock().await.jobs.contains_key(&request.job_id) {
            return Err(missing_job(request.job_id));
        }
        Ok(JobArtifactListResult {
            artifacts: Vec::new(),
            limit: clamp_page_limit(request.limit),
            next_cursor: None,
        })
    }

    async fn reset(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.jobs.clear();
        state.requests.clear();
        state.stages.clear();
        state.events.clear();
        state.idempotency_keys.clear();
        state.next_job = 0;
        state.next_stage = 0;
        Ok(())
    }

    async fn capabilities(&self) -> Result<JobStoreCapability> {
        Ok(capability("fake-job-store").into())
    }
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
