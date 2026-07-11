use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_observe::sink::SqliteObservabilitySink;
use sqlx::SqlitePool;

use crate::boundary::{JobDeleteResult, JobStore, Result};
pub use crate::store_inventory::{
    LegacyJobStoreBlocker, RECEIPT_KIND_LEGACY_RESET, RECEIPT_KIND_PREFLIGHT_CLEAN_CUTOVER,
    detect_incompatible_legacy_jobs, record_cutover_receipt,
};

#[path = "unified/control.rs"]
mod control;
#[path = "unified/control_helpers.rs"]
mod control_helpers;
#[path = "unified/heartbeat.rs"]
mod heartbeat;
#[path = "unified/observe.rs"]
mod observe;
#[path = "unified/ops.rs"]
mod ops;
#[path = "unified/pagination.rs"]
mod pagination;
#[path = "unified/request_read.rs"]
mod request_read;
#[path = "unified/retention.rs"]
pub(crate) mod retention;
#[path = "unified/schema.rs"]
mod schema;

#[derive(Clone)]
pub struct SqliteUnifiedJobStore {
    pool: SqlitePool,
    /// Optional durable observability sink (`axon_observe_events`/heartbeats).
    ///
    /// When present, every status transition and heartbeat routed through this
    /// store is *also* recorded as a durable [`SourceProgressEvent`] with a
    /// strictly-increasing per-`job_id` sequence. This supplements — it never
    /// replaces — the existing `job_events`/`progress_json` streams that back
    /// SSE/status rendering, so streaming behavior is unchanged. `None` (the
    /// bare [`SqliteUnifiedJobStore::new`] constructor, used by fakes/tests)
    /// disables the supplement entirely.
    observe: Option<Arc<SqliteObservabilitySink>>,
}

/// Maximum bounded provider-cooling window.
///
/// [`SqliteUnifiedJobStore::apply_provider_cooling`] clamps any incoming
/// `ProviderCooling.cooldown_until` to `min(cooldown_until, now + this)`
/// before persisting. A fixed conservative bound is the point: an unbounded
/// or attacker/bug-supplied far-future timestamp must not be able to
/// permanently blacklist a job kind from ever being claimed again (flagged as
/// a DoS-shaped risk in engineering review). Not configurable by design.
pub const MAX_PROVIDER_COOLDOWN_WINDOW: std::time::Duration = std::time::Duration::from_secs(3600);

impl SqliteUnifiedJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            observe: None,
        }
    }

    /// Build a store that also routes status/heartbeat transitions into the
    /// durable observability sink on the same pool.
    pub fn with_observe_sink(pool: SqlitePool, observe: Arc<SqliteObservabilitySink>) -> Self {
        Self {
            pool,
            observe: Some(observe),
        }
    }

    #[cfg(test)]
    pub(crate) fn pool_for_tests(&self) -> &SqlitePool {
        &self.pool
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

    async fn request_json(&self, job_id: JobId) -> Result<Option<serde_json::Value>> {
        self.get_job_request_json(job_id).await
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

    async fn delete_jobs(&self, job_ids: &[JobId]) -> Result<JobDeleteResult> {
        self.delete_job_rows(job_ids).await
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
