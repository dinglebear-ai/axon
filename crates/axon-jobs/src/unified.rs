use async_trait::async_trait;
use axon_api::source::*;
use sqlx::SqlitePool;

use crate::boundary::{JobStore, Result};

#[path = "unified/control.rs"]
mod control;
#[path = "unified/control_helpers.rs"]
mod control_helpers;
#[path = "unified/heartbeat.rs"]
mod heartbeat;
#[path = "unified/ops.rs"]
mod ops;

#[derive(Debug, Clone)]
pub struct SqliteUnifiedJobStore {
    pool: SqlitePool,
}

impl SqliteUnifiedJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobStore for SqliteUnifiedJobStore {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor> {
        self.create_job(request).await
    }

    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        self.get_job(job_id).await
    }

    async fn attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>> {
        self.job_attempts(job_id).await
    }

    async fn stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>> {
        self.job_stages(job_id).await
    }

    async fn update_status(&self, status: JobStatusUpdate) -> Result<()> {
        self.update_job_status(status).await
    }

    async fn append_event(&self, event: SourceProgressEvent) -> Result<()> {
        self.append_job_event(event).await
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        self.record_heartbeat(heartbeat).await
    }

    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        self.list_jobs(request).await
    }

    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage> {
        self.list_events(request).await
    }

    async fn latest_event_sequence(&self, job_id: JobId) -> Result<Option<u64>> {
        self.latest_sequence(job_id).await
    }

    async fn cancel(&self, job_id: JobId, request: JobCancelRequest) -> Result<JobCancelResult> {
        self.cancel_job(job_id, request).await
    }

    async fn retry(&self, job_id: JobId, request: JobRetryRequest) -> Result<JobRetryResult> {
        self.retry_job(job_id, request).await
    }

    async fn recover(&self, request: JobRecoveryRequest) -> Result<JobRecoveryResult> {
        self.recover_jobs(request).await
    }

    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult> {
        self.cleanup_jobs(request).await
    }

    async fn artifacts(&self, request: JobArtifactListRequest) -> Result<JobArtifactListResult> {
        self.list_job_artifacts(request).await
    }

    async fn reset(&self) -> Result<()> {
        self.reset_jobs().await
    }

    async fn capabilities(&self) -> Result<JobStoreCapability> {
        self.store_capabilities().await
    }
}

#[cfg(test)]
#[path = "unified_tests.rs"]
mod tests;
