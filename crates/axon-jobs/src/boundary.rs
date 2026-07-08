use async_trait::async_trait;
use axon_api::source::*;

pub use crate::fake_store::FakeJobWatchStore;

pub type Result<T> = std::result::Result<T, ApiError>;

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
