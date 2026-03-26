pub mod full;
pub mod lite;

use async_trait::async_trait;
use uuid::Uuid;

use crate::crates::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::crates::services::types::ServiceJob;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Started,
    InProcess,
    Unsupported(&'static str),
}

#[async_trait]
pub trait ServiceJobRuntime: Send + Sync {
    fn mode_name(&self) -> &'static str;

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid>;

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String>;

    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>>;

    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool>;

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn std::error::Error>>;

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn std::error::Error>>;

    async fn cancel_job(&self, kind: JobKind, id: Uuid)
    -> Result<bool, Box<dyn std::error::Error>>;

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn std::error::Error>>;

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn std::error::Error>>;

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn std::error::Error>>;

    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn std::error::Error>>;
}
