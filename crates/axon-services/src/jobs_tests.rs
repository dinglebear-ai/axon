use super::*;
use crate::runtime::ServiceJobRuntime;
use async_trait::async_trait;
use axon_api::source::{
    JobClearRequest, JobEventListRequest, JobExecutionMode, JobListRequest, JobPolicy,
    JobStatusUpdate, LifecycleStatus, OperationKind, PipelinePhase, Severity, SourceProgressEvent,
    StageCounts, Timestamp, Visibility, job_policy_for_operation,
};
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use axon_jobs::boundary::JobStore;
use axon_jobs::store::open_sqlite_pool;
use axon_jobs::unified::SqliteUnifiedJobStore;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct CaptureRuntime {
    seen_filters: Mutex<Vec<Option<String>>>,
    unified: Option<Arc<dyn JobStore>>,
}

#[async_trait]
impl ServiceJobRuntime for CaptureRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    fn unified_job_store(&self) -> Option<Arc<dyn JobStore>> {
        self.unified.clone()
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        Err("not implemented".into())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Err("not implemented".into())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        self.seen_filters
            .lock()
            .expect("lock")
            .push(source_filter.map(str::to_string));
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

#[tokio::test]
async fn list_ingest_jobs_delegates_source_filter_to_runtime() {
    let cfg = Arc::new(axon_core::config::Config::default());
    let runtime = Arc::new(CaptureRuntime {
        seen_filters: Mutex::new(Vec::new()),
        unified: None,
    });
    let ctx = ServiceContext::from_runtime(cfg, runtime.clone());

    let jobs = list_ingest_jobs(&ctx, Some("sessions"), 50, 0)
        .await
        .expect("list should succeed");
    assert!(jobs.is_empty());
    assert_eq!(
        runtime.seen_filters.lock().expect("lock").as_slice(),
        &[Some("sessions".to_string())]
    );
}

#[tokio::test]
async fn every_job_backed_operation_creates_unified_job_descriptor() {
    let ctx = test_context_with_unified_jobs().await;

    for operation in [
        OperationKind::Source,
        OperationKind::Watch,
        OperationKind::Extract,
        OperationKind::Research,
        OperationKind::MemoryCompaction,
        OperationKind::MemoryImport,
        OperationKind::GraphMutation,
        OperationKind::Prune,
        OperationKind::ProviderProbe,
        OperationKind::Reset,
    ] {
        let descriptor = enqueue_operation(
            &ctx,
            operation,
            JobExecutionMode::Detached,
            serde_json::json!({"operation": format!("{operation:?}")}),
        )
        .await
        .expect("enqueue")
        .expect("job-backed operation should return descriptor");

        assert_eq!(descriptor.status, LifecycleStatus::Queued);
        assert!(descriptor.poll_after_ms > 0);
        assert!(descriptor.status_url.starts_with("/v1/jobs/"));
        assert!(descriptor.events_url.ends_with("/events"));
    }

    let page = list_unified_jobs(&ctx, empty_job_list_request())
        .await
        .expect("list jobs");
    assert_eq!(page.items.len(), 10);
}

#[tokio::test]
async fn normal_query_and_retrieve_do_not_create_jobs() {
    let ctx = test_context_with_unified_jobs().await;

    assert_eq!(
        job_policy_for_operation(OperationKind::Query, JobExecutionMode::Foreground),
        JobPolicy::Synchronous
    );
    assert!(
        enqueue_operation(
            &ctx,
            OperationKind::Query,
            JobExecutionMode::Foreground,
            serde_json::json!({"query": "hello"}),
        )
        .await
        .expect("query policy")
        .is_none()
    );
    assert!(
        enqueue_operation(
            &ctx,
            OperationKind::Retrieve,
            JobExecutionMode::Foreground,
            serde_json::json!({"url": "https://example.com"}),
        )
        .await
        .expect("retrieve policy")
        .is_none()
    );

    let page = list_unified_jobs(&ctx, empty_job_list_request())
        .await
        .expect("list jobs");
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn unified_job_events_after_sequence_uses_service_event_page() {
    let ctx = test_context_with_unified_jobs().await;
    let descriptor = enqueue_operation(
        &ctx,
        OperationKind::Source,
        JobExecutionMode::Detached,
        serde_json::json!({"operation": "source"}),
    )
    .await
    .expect("enqueue")
    .expect("job descriptor");
    let store = ctx.job_store().expect("job store");

    for sequence in 1..=3 {
        store
            .append_event(progress_event(descriptor.job_id, sequence))
            .await
            .expect("append event");
    }

    let page = unified_job_events(
        &ctx,
        JobEventListRequest {
            job_id: descriptor.job_id,
            after_sequence: Some(1),
            limit: Some(2),
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
            cursor: None,
        },
    )
    .await
    .expect("event page");

    assert_eq!(
        page.events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(page.last_sequence, 3);
}

#[tokio::test]
async fn clear_unified_jobs_requires_confirm_and_drains_terminal_batches() {
    let ctx = test_context_with_unified_jobs().await;
    let store = ctx.job_store().expect("job store");

    let denied = clear_unified_jobs(
        &ctx,
        JobClearRequest {
            status: None,
            confirm: false,
            kind: None,
            older_than: None,
        },
    )
    .await;
    assert!(denied.is_err());

    for index in 0..105 {
        let descriptor = enqueue_operation(
            &ctx,
            OperationKind::Source,
            JobExecutionMode::Detached,
            serde_json::json!({"operation": "source", "index": index}),
        )
        .await
        .expect("enqueue")
        .expect("job descriptor");
        store
            .update_status(JobStatusUpdate {
                source_id: None,
                job_id: descriptor.job_id,
                status: LifecycleStatus::Running,
                phase: PipelinePhase::Planning,
                stage_id: None,
                counts: None,
                current: None,
                message: None,
                error: None,
            })
            .await
            .expect("run job");
        store
            .update_status(JobStatusUpdate {
                source_id: None,
                job_id: descriptor.job_id,
                status: LifecycleStatus::Completed,
                phase: PipelinePhase::Complete,
                stage_id: None,
                counts: None,
                current: None,
                message: None,
                error: None,
            })
            .await
            .expect("complete job");
    }
    let active = enqueue_operation(
        &ctx,
        OperationKind::Source,
        JobExecutionMode::Detached,
        serde_json::json!({"operation": "source", "active": true}),
    )
    .await
    .expect("enqueue active")
    .expect("active job descriptor");

    let cleared = clear_unified_jobs(
        &ctx,
        JobClearRequest {
            status: None,
            confirm: true,
            kind: None,
            older_than: None,
        },
    )
    .await
    .expect("clear");
    assert_eq!(cleared.deleted, 105);

    let remaining = unified_job_status(&ctx, active.job_id)
        .await
        .expect("status")
        .expect("active remains");
    assert_eq!(remaining.status, LifecycleStatus::Queued);
}

async fn test_context_with_unified_jobs() -> ServiceContext {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    let store: Arc<dyn JobStore> = Arc::new(SqliteUnifiedJobStore::new(pool));
    let runtime = Arc::new(CaptureRuntime {
        seen_filters: Mutex::new(Vec::new()),
        unified: Some(store),
    });
    ServiceContext::from_runtime(Arc::new(axon_core::config::Config::default()), runtime)
}

fn empty_job_list_request() -> JobListRequest {
    JobListRequest {
        status: None,
        kind: None,
        source_id: None,
        watch_id: None,
        limit: None,
        cursor: None,
    }
}

fn progress_event(job_id: axon_api::source::JobId, sequence: u64) -> SourceProgressEvent {
    SourceProgressEvent {
        event_id: format!("event-{sequence}"),
        sequence,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Fetching,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility: Visibility::Public,
        message: format!("event {sequence}"),
        timestamp: Timestamp("2026-07-05T00:00:00Z".to_string()),
        source_id: None,
        canonical_uri: None,
        adapter: None,
        scope: None,
        generation: None,
        counts: StageCounts {
            items_total: Some(3),
            items_done: sequence,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        timing: None,
        current: None,
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    }
}
