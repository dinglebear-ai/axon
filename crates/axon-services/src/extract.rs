//! Service-layer wrappers for extract job lifecycle operations and prompt-aware enqueue helpers.

mod sync;

use crate::context::ServiceContext;
use crate::events::ServiceEvent;
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ExecutionMode, ExtractJobResult, ExtractStartResult, JobStartOutcome, StartDisposition,
};
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobIntent, JobKind, JobPriority, JobStagePlan, MetadataMap,
    PipelinePhase,
};
use axon_core::config::Config;
use axon_jobs::config_snapshot::extract_config_json;
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_extract_start_result(job_id: String) -> ExtractStartResult {
    ExtractStartResult { job_id }
}

pub fn map_extract_job_result(payload: serde_json::Value) -> ExtractJobResult {
    ExtractJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn extract_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<ExtractJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Extract, id).await?;
    Ok(job.map(|value| {
        map_extract_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn extract_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<ExtractJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Extract, limit, offset).await?;
    Ok(map_extract_job_result(serde_json::to_value(jobs)?))
}

pub async fn extract_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Extract, id).await
}

pub async fn extract_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Extract).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

pub async fn extract_start_with_context(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
    service_context: &ServiceContext,
    _tx: Option<mpsc::Sender<ServiceEvent>>,
    caller: Option<&AuthSnapshot>,
) -> Result<JobStartOutcome<ExtractStartResult>, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("extract_start requires at least one URL".into());
    }

    // Extract jobs create a real row on the unified JobStore and execute via
    // the unified worker.
    let config_json = extract_config_json(cfg, prompt.or_else(|| cfg.query.clone()))?;
    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Extract,
            job_intent: JobIntent::Extract,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![JobStagePlan {
                phase: PipelinePhase::Parsing,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: Some(urls.len() as u64),
            }],
            request: Some(serde_json::json!({
                "urls": urls,
                "config_json": config_json,
            })),
            auth_snapshot: caller
                .cloned()
                .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("extract_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e.message.into() })?;
    service_context.notify_unified();
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_extract_start_result(descriptor.job_id.0.to_string()),
    })
}

// --- Sync extract (--wait true) ---

pub use sync::extract_sync;
