//! `JobService` — unified job lifecycle (get/list/events/cancel/retry/
//! recover/cleanup/clear).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §JobService. Every method here wraps a `crates/axon-services/src/jobs/
//! unified_ops.rs` free function 1:1 (all DTOs already match); this is the
//! highest-confidence trait in the whole service-trait seam since
//! `unified_ops.rs` already speaks the exact `axon-api` job DTOs.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobCleanupResult, JobClearRequest, JobClearResult,
    JobDescriptor, JobEventListRequest, JobEventPage, JobId, JobKind, JobListRequest,
    JobRecoverRequest, JobRecoverResult, JobRetryMode, JobRetryRequest, JobSummary,
    LifecycleStatus, MetadataMap, Page, PipelinePhase, Timestamp,
};

use crate::context::ServiceContext;
use crate::jobs as unified_ops;

#[async_trait]
pub trait JobService: Send + Sync {
    async fn get(&self, job_id: JobId) -> anyhow::Result<JobSummary>;
    async fn list(&self, request: JobListRequest) -> anyhow::Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> anyhow::Result<JobEventPage>;
    async fn cancel(&self, job_id: JobId) -> anyhow::Result<JobSummary>;
    async fn retry(&self, job_id: JobId) -> anyhow::Result<JobDescriptor>;
    async fn recover(&self, request: JobRecoverRequest) -> anyhow::Result<JobRecoverResult>;
    async fn cleanup(&self, request: JobCleanupRequest) -> anyhow::Result<JobCleanupResult>;
    async fn clear(&self, request: JobClearRequest) -> anyhow::Result<JobClearResult>;
}

/// Production implementation: thin delegation to `jobs::unified_ops`.
pub struct JobServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl JobServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl JobService for JobServiceImpl {
    async fn get(&self, job_id: JobId) -> anyhow::Result<JobSummary> {
        unified_ops::unified_job_status(&self.ctx, job_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .ok_or_else(|| anyhow::anyhow!("job {} not found", job_id.0))
    }

    async fn list(&self, request: JobListRequest) -> anyhow::Result<Page<JobSummary>> {
        unified_ops::list_unified_jobs(&self.ctx, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn events(&self, request: JobEventListRequest) -> anyhow::Result<JobEventPage> {
        unified_ops::unified_job_events(&self.ctx, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn cancel(&self, job_id: JobId) -> anyhow::Result<JobSummary> {
        let request = JobCancelRequest {
            reason: None,
            force_after_ms: None,
        };
        unified_ops::cancel_unified_job(&self.ctx, job_id, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.get(job_id).await
    }

    async fn retry(&self, job_id: JobId) -> anyhow::Result<JobDescriptor> {
        let request = JobRetryRequest {
            mode: JobRetryMode::SameConfig,
            from_phase: None,
            idempotency_key: None,
            overrides: MetadataMap::new(),
        };
        let result = unified_ops::retry_unified_job(&self.ctx, job_id, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(result.retry_job)
    }

    async fn recover(&self, request: JobRecoverRequest) -> anyhow::Result<JobRecoverResult> {
        unified_ops::recover_unified_jobs(&self.ctx, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn cleanup(&self, request: JobCleanupRequest) -> anyhow::Result<JobCleanupResult> {
        unified_ops::cleanup_unified_jobs(&self.ctx, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn clear(&self, request: JobClearRequest) -> anyhow::Result<JobClearResult> {
        unified_ops::clear_unified_jobs(&self.ctx, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn fake_job_summary(job_id: JobId, status: LifecycleStatus) -> JobSummary {
    let now = Timestamp::from(chrono::Utc::now());
    JobSummary {
        job_id,
        kind: JobKind::Source,
        status,
        phase: PipelinePhase::Queued,
        created_at: now.clone(),
        updated_at: now,
        source_id: None,
        watch_id: None,
        intent: None,
        started_at: None,
        finished_at: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 0,
        priority: axon_api::source::JobPriority::Normal,
        counts: None,
        current: None,
        heartbeat: None,
        last_error: None,
        warnings: Vec::new(),
    }
}

/// Deterministic in-memory fake covering every `JobService` method.
#[derive(Default)]
pub struct FakeJobService {
    jobs: Mutex<std::collections::HashMap<uuid::Uuid, JobSummary>>,
}

impl FakeJobService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a job the fake will recognize for `get`/`cancel`/`retry`.
    pub fn seed(&self, job: JobSummary) {
        self.jobs.lock().unwrap().insert(job.job_id.0, job);
    }
}

#[async_trait]
impl JobService for FakeJobService {
    async fn get(&self, job_id: JobId) -> anyhow::Result<JobSummary> {
        self.jobs
            .lock()
            .unwrap()
            .get(&job_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("job {} not found", job_id.0))
    }

    async fn list(&self, request: JobListRequest) -> anyhow::Result<Page<JobSummary>> {
        let jobs = self.jobs.lock().unwrap();
        let limit = request.limit.unwrap_or(50);
        Ok(Page {
            items: jobs.values().take(limit as usize).cloned().collect(),
            next_cursor: None,
            limit,
            total: Some(jobs.len() as u64),
        })
    }

    async fn events(&self, _request: JobEventListRequest) -> anyhow::Result<JobEventPage> {
        Ok(JobEventPage {
            events: Vec::new(),
            next_cursor: None,
            last_sequence: 0,
            limit: 0,
        })
    }

    async fn cancel(&self, job_id: JobId) -> anyhow::Result<JobSummary> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(&job_id.0)
            .ok_or_else(|| anyhow::anyhow!("job {} not found", job_id.0))?;
        job.status = LifecycleStatus::Canceled;
        Ok(job.clone())
    }

    async fn retry(&self, job_id: JobId) -> anyhow::Result<JobDescriptor> {
        self.jobs
            .lock()
            .unwrap()
            .insert(job_id.0, fake_job_summary(job_id, LifecycleStatus::Queued));
        Ok(JobDescriptor {
            kind: JobKind::Source,
            id: job_id,
            status_url: format!("/v1/jobs/{}", job_id.0),
            events_url: format!("/v1/jobs/{}/events", job_id.0),
            stream_url: format!("/v1/jobs/{}/stream", job_id.0),
            poll_after_ms: 1_000,
            cancel_url: None,
            retry_url: None,
            job_id,
            status: LifecycleStatus::Queued,
            poll: None,
            created_at: None,
            updated_at: None,
        })
    }

    async fn recover(&self, _request: JobRecoverRequest) -> anyhow::Result<JobRecoverResult> {
        Ok(JobRecoverResult {
            recovered: 0,
            job_ids: Vec::new(),
            warnings: Vec::new(),
            jobs_scanned: 0,
            jobs_requeued: 0,
            jobs_failed: 0,
        })
    }

    async fn cleanup(&self, request: JobCleanupRequest) -> anyhow::Result<JobCleanupResult> {
        Ok(JobCleanupResult {
            matched: 0,
            deleted: 0,
            dry_run: request.dry_run,
            warnings: Vec::new(),
            jobs_pruned: 0,
            events_pruned: 0,
            heartbeats_pruned: 0,
            attempts_pruned: 0,
            stages_pruned: 0,
            reservations_pruned: 0,
            artifacts_pruned: 0,
        })
    }

    async fn clear(&self, request: JobClearRequest) -> anyhow::Result<JobClearResult> {
        if !request.confirm {
            anyhow::bail!("job clear requires confirm=true");
        }
        let mut jobs = self.jobs.lock().unwrap();
        let deleted = jobs.len() as u64;
        jobs.clear();
        Ok(JobClearResult {
            deleted,
            status: request.status,
            warnings: Vec::new(),
        })
    }
}

#[cfg(test)]
#[path = "job_service_tests.rs"]
mod tests;
