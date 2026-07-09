use crate::context::ServiceContext;
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobListResult,
    JobStartOutcome, StartDisposition,
};
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    JobStagePlan, MetadataMap, PipelinePhase,
};
use axon_core::config::Config;
use axon_jobs::backend::JobKind;
use axon_jobs::config_snapshot::ingest_config_json;
use axon_jobs::ingest::types::{source_type_label, target_label};
pub use axon_jobs::ingest::{IngestJob, IngestSource};
use axon_jobs::ingest::{count_ingest_jobs, get_ingest_job, list_ingest_jobs};
use std::error::Error;
use uuid::Uuid;

pub mod classify;
mod prepared_sessions;
pub mod request;
pub use axon_ingest::orchestrate::{
    ingest_payload, ingest_sessions, ingest_sessions_prepared_with_progress,
    ingest_sessions_with_progress, map_ingest_result,
};
pub use classify::classify_target;
pub use prepared_sessions::ingest_sessions_prepared_start_with_context;
pub use request::{source_from_mcp_request, validate_ingest_source};

pub fn map_ingest_start_result(job_id: String) -> IngestStartResult {
    IngestStartResult { job_id }
}

pub fn map_ingest_job_result(payload: serde_json::Value) -> IngestJobResult {
    IngestJobResult { payload }
}

// --- Service lifecycle wrappers ---

/// Pre-flight existence check run before an ingest job is enqueued.
///
/// Phase 12 clean break (issue #298): the GitHub existence probe this used to
/// run was deleted along with `axon-ingest`'s provider orchestration — every
/// non-session `IngestSource` now fails at execution time in the legacy job
/// runner instead, so there is nothing left to preflight here.
pub async fn preflight_ingest_source(
    _cfg: &Config,
    _source: &IngestSource,
) -> Result<(), Box<dyn Error>> {
    Ok(())
}

pub async fn ingest_start_with_context(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
    caller: Option<&AuthSnapshot>,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    preflight_ingest_source(cfg, &source).await?;
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    let config_json = ingest_config_json(cfg, &source)?;
    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: UnifiedJobKind::Ingest,
            job_intent: JobIntent::Run,
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
                estimated_items: None,
            }],
            request: Some(serde_json::json!({
                "source": source,
                "source_type": source_type,
                "target": target,
                "config_json": config_json,
            })),
            auth_snapshot: caller
                .cloned()
                .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("ingest_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e.message.into() })?;
    service_context.notify_unified();
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(descriptor.job_id.0.to_string()),
    })
}

pub async fn ingest_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Ingest, id).await?;
    Ok(job.map(|value| {
        map_ingest_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn ingest_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<IngestResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Ingest, limit, offset).await?;
    Ok(map_ingest_result(serde_json::to_value(jobs)?))
}

pub async fn ingest_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Ingest, id).await
}

pub async fn ingest_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_count(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    count_ingest_jobs(cfg).await
}

pub async fn ingest_status_raw(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<IngestJob>, Box<dyn Error>> {
    get_ingest_job(cfg, id).await
}

pub async fn ingest_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<IngestJob>, Box<dyn Error>> {
    let (jobs, total) = tokio::join!(
        list_ingest_jobs(cfg, None, limit, offset),
        count_ingest_jobs(cfg),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}

pub async fn ingest_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Ingest).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
