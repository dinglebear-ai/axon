use async_trait::async_trait;
use axon_api::source::*;

pub use crate::fake_store::FakeJobWatchStore;
pub use crate::watch_store::SqliteWatchStore;

pub type Result<T> = std::result::Result<T, ApiError>;

/// Outcome of a scoped, per-job-id delete ([`JobStore::delete_jobs`]).
///
/// This is an internal maintenance-path type, not a wire DTO — unlike
/// [`JobCleanupResult`] (bulk, age/kind-scoped retention, already exposed on
/// CLI/REST/MCP), `delete_jobs` targets an explicit, caller-supplied id set
/// and today is only consumed in-process by the `axon-services` cleanup-debt
/// drain (`CleanupDebtKind::JobRetention` / `CleanupSelector::JobRows`), so it
/// lives here rather than in `axon-api` (mirroring `JobStatusRow`/
/// `JobSummary` in `backend.rs`, which are also internal-only shapes).
///
/// A row only ever moves into `deleted` when its status is terminal at the
/// moment of the call — deleting a `running`/`waiting`/`canceling`/etc. row
/// out from under its worker is unsafe, so such rows land in `skipped_live`
/// instead. `missing` covers ids that named no row at all (already deleted,
/// or never existed) — never an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobDeleteResult {
    /// Job ids whose row (and cascaded child rows — events, heartbeats,
    /// attempts, stages, artifacts, provider reservations) was deleted.
    pub deleted: Vec<JobId>,
    /// Job ids that exist but are not in a terminal status; left untouched.
    pub skipped_live: Vec<JobId>,
    /// Job ids that named no existing row.
    pub missing: Vec<JobId>,
}

#[async_trait]
pub trait JobStore: Send + Sync {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor>;
    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>>;
    /// The raw `request` payload captured at `create()` time (e.g. the
    /// `{"urls": [...], "config_json": "..."}` shape `JobKind::Extract`
    /// stores). `JobSummary` intentionally does not carry this -- it is a
    /// shared, transport-facing projection reused by every job kind -- so
    /// callers that need to redisplay the original request (like the
    /// Extract CLI/MCP/REST bridge) fetch it separately via this method.
    /// Default implementation returns `Ok(None)` for stores that do not
    /// persist request payloads.
    async fn request_json(&self, _job_id: JobId) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
    async fn attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>>;
    async fn stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>>;
    async fn update_status(&self, status: JobStatusUpdate) -> Result<()>;
    async fn append_event(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage>;
    async fn latest_event_sequence(&self, job_id: JobId) -> Result<Option<u64>>;
    async fn cancel(&self, job_id: JobId, request: JobCancelRequest) -> Result<JobCancelResult>;
    async fn retry(&self, job_id: JobId, request: JobRetryRequest) -> Result<JobRetryResult>;
    async fn recover(&self, request: JobRecoveryRequest) -> Result<JobRecoveryResult>;
    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult>;
    /// Delete specific job rows (and their cascaded child rows) by id,
    /// refusing any row not currently in a terminal status. Unlike
    /// [`JobStore::cleanup`] (bulk, age/kind-scoped), this targets an
    /// explicit, caller-supplied id set — see [`JobDeleteResult`].
    async fn delete_jobs(&self, job_ids: &[JobId]) -> Result<JobDeleteResult>;
    async fn artifacts(&self, request: JobArtifactListRequest) -> Result<JobArtifactListResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<JobStoreCapability>;
}

#[async_trait]
pub trait WatchStore: Send + Sync {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult>;
    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>>;
    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>>;
    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()>;
    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<WatchStoreCapability>;
}
