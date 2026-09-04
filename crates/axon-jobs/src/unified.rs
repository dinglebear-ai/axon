use std::sync::Arc;

#[cfg(test)]
mod snapshot_test_hook {
    use axon_api::source::JobId;
    use std::sync::{Arc, Mutex, OnceLock};
    use tokio::sync::Notify;

    pub(super) struct Hook {
        pub job_id: JobId,
        pub entered: Arc<Notify>,
        pub resume: Arc<Notify>,
    }

    static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();

    pub(super) fn install(job_id: JobId) -> (Arc<Notify>, Arc<Notify>) {
        let entered = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        *HOOK
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("hook lock") = Some(Hook {
            job_id,
            entered: Arc::clone(&entered),
            resume: Arc::clone(&resume),
        });
        (entered, resume)
    }

    pub(super) async fn pause_once_after_read(job_id: JobId) {
        let hook = {
            let mut guard = HOOK
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("hook lock");
            if guard.as_ref().is_some_and(|hook| hook.job_id == job_id) {
                guard.take()
            } else {
                None
            }
        };
        if let Some(hook) = hook {
            hook.entered.notify_one();
            hook.resume.notified().await;
        }
    }
}

use async_trait::async_trait;
use axon_api::source::*;
use axon_observe::sink::SqliteObservabilitySink;
use sqlx::SqlitePool;

use crate::boundary::{JobDeleteResult, JobStore, Result};

#[path = "unified/artifacts.rs"]
mod artifacts;
#[path = "unified/control.rs"]
mod control;
#[path = "unified/control_helpers.rs"]
mod control_helpers;
#[path = "unified/event_listing.rs"]
mod event_listing;
#[path = "unified/event_ops.rs"]
mod event_ops;
#[path = "unified/heartbeat.rs"]
mod heartbeat;
#[path = "unified/observe.rs"]
mod observe;
#[path = "unified/ops.rs"]
mod ops;
#[path = "unified/ops_helpers.rs"]
mod ops_helpers;
#[path = "unified/pagination.rs"]
pub(crate) mod pagination;
#[path = "unified/projection_admission.rs"]
mod projection_admission;
#[path = "unified/recovery.rs"]
mod recovery;
#[path = "unified/request_read.rs"]
mod request_read;
#[path = "unified/retention.rs"]
pub(crate) mod retention;
#[path = "unified/schema.rs"]
mod schema;
#[path = "unified/terminal_counts.rs"]
mod terminal_counts;
#[path = "unified/terminal_warnings.rs"]
mod terminal_warnings;

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
        retry_job_write("job create", || self.create_job(request.clone())).await
    }

    async fn create_with_config_snapshot(
        &self,
        request: JobCreateRequest,
        config_json: Option<&str>,
    ) -> Result<JobDescriptor> {
        retry_job_write("job create with config snapshot", || {
            self.create_job_with_snapshot(request.clone(), config_json)
        })
        .await
    }

    async fn admit_projection_batch_atomic(
        &self,
        admission: ProjectionBatchAdmission,
    ) -> Result<ProjectionBatchAdmissionResult> {
        retry_job_write("projection batch admission", || {
            self.admit_projection_batch(admission.clone())
        })
        .await
    }

    async fn projection_batch(
        &self,
        lookup: ProjectionBatchLookup,
    ) -> Result<Option<ProjectionBatchAdmissionResult>> {
        self.lookup_projection_batch(lookup).await
    }

    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        self.get_job(job_id).await
    }

    async fn request_json(&self, job_id: JobId) -> Result<Option<serde_json::Value>> {
        self.get_job_request_json(job_id).await
    }

    async fn result_json(&self, job_id: JobId) -> Result<Option<serde_json::Value>> {
        self.get_job_result_json(job_id).await
    }

    async fn attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>> {
        self.job_attempts(job_id).await
    }

    async fn stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>> {
        self.job_stages(job_id).await
    }

    async fn update_status(&self, status: JobStatusUpdate) -> Result<()> {
        retry_job_write("job status update", || {
            self.update_job_status(status.clone())
        })
        .await
    }

    async fn append_event(&self, event: SourceProgressEvent) -> Result<()> {
        // Same rationale as `heartbeat`: progress events are emitted on every
        // pipeline phase transition, and were the other write observed failing
        // with 517 once the container and host CLI shared jobs.db.
        retry_job_write("job progress event", || {
            self.append_job_event(event.clone())
        })
        .await
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        // Retried on a transient busy condition. `record_heartbeat` opens and
        // commits its own transaction, so re-running it is atomic. Heartbeats
        // are the highest-frequency write in the store and were the first thing
        // to fail with SQLITE_BUSY_SNAPSHOT (517) once a second process shared
        // the database — and 517 is precisely what `busy_timeout` cannot cover.
        retry_job_write("job heartbeat", || self.record_heartbeat(heartbeat.clone())).await
    }

    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        self.list_jobs(request).await
    }

    async fn terminal_counts(&self, job_id: JobId) -> Result<Option<StageCounts>> {
        self.terminal_counts_from_events(job_id).await
    }

    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage> {
        self.list_events(request).await
    }

    async fn latest_event_sequence(&self, job_id: JobId) -> Result<Option<u64>> {
        self.latest_sequence(job_id).await
    }

    async fn cancel(&self, job_id: JobId, request: JobCancelRequest) -> Result<JobCancelResult> {
        retry_job_write("job cancel", || self.cancel_job(job_id, request.clone())).await
    }

    async fn retry(&self, job_id: JobId, request: JobRetryRequest) -> Result<JobRetryResult> {
        retry_job_write("job retry", || self.retry_job(job_id, request.clone())).await
    }

    async fn recover(&self, request: JobRecoveryRequest) -> Result<JobRecoveryResult> {
        self.recover_jobs(request).await
    }

    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult> {
        retry_job_write("job cleanup", || self.cleanup_jobs(request.clone())).await
    }

    async fn delete_jobs(&self, job_ids: &[JobId]) -> Result<JobDeleteResult> {
        let job_ids = job_ids.to_vec();
        retry_job_write("job delete", || self.delete_job_rows(&job_ids)).await
    }

    async fn artifacts(&self, request: JobArtifactListRequest) -> Result<JobArtifactListResult> {
        self.list_job_artifacts(request).await
    }

    async fn reset(&self) -> Result<()> {
        retry_job_write("job reset", || self.reset_jobs()).await
    }

    async fn capabilities(&self) -> Result<JobStoreCapability> {
        self.store_capabilities().await
    }
}

/// Retry every `JobStore` mutation at the public store boundary. Each delegated
/// operation owns one SQLite transaction (or is idempotent); restarting the
/// whole boundary operation discards a stale WAL snapshot instead of trying to
/// continue it. Keep this wrapper here rather than at individual SQL calls so
/// new write paths cannot silently miss `SQLITE_BUSY_SNAPSHOT` coverage.
pub(crate) async fn retry_job_write<T, F, Fut>(what: &str, op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    axon_core::sqlite::retry_on(
        what,
        |e: &ApiError| axon_core::sqlite::message_is_retryable_busy(&e.to_string()),
        op,
    )
    .await
}

#[cfg(test)]
#[path = "unified_tests.rs"]
mod tests;
