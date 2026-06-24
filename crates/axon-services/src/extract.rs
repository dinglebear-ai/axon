//! Service-layer wrappers for extract job lifecycle operations and prompt-aware enqueue helpers.

use crate::context::ServiceContext;
use crate::events::{LogLevel, ServiceEvent, emit};
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ExecutionMode, ExtractJobResult, ExtractStartResult, JobStartOutcome, StartDisposition,
};
use axon_core::config::Config;
use axon_jobs::backend::{JobKind, JobPayload};
use axon_jobs::config_snapshot::extract_config_json;
use axon_jobs::extract::start_extract_job;
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

/// Enqueue an extract job for the given URLs and return its job ID immediately.
/// The extract prompt is read from cfg.query if present.
pub async fn extract_start(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ExtractStartResult, Box<dyn Error>> {
    extract_start_with_prompt(cfg, urls, cfg.query.clone(), tx).await
}

pub async fn extract_start_with_prompt(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ExtractStartResult, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("extract_start requires at least one URL".into());
    }

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing extract job for {} URL(s)", urls.len()),
        },
    )
    .await;

    let job_id = start_extract_job(cfg, urls.to_vec(), prompt).await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueued extract job: {job_id}"),
        },
    )
    .await;

    Ok(map_extract_start_result(job_id.to_string()))
}

pub async fn extract_start_with_context(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
    service_context: &ServiceContext,
    _tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<ExtractStartResult>, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("extract_start requires at least one URL".into());
    }

    // Always route through service_context.jobs.enqueue() so that notify()
    // fires immediately and workers wake without 0-5 second polling delay.
    let config_json = extract_config_json(cfg, prompt.or_else(|| cfg.query.clone()))?;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Extract {
            urls: urls.to_vec(),
            config_json,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_extract_start_result(job_id.to_string()),
    })
}

// --- Sync extract (--wait true) ---

/// Synchronous structured extraction now lives in `axon_extract::sync`; re-exported
/// so existing `crate::extract::extract_sync` call sites resolve unchanged.
pub use axon_extract::sync::extract_sync;
