//! Service-layer wrappers for extract job lifecycle operations and prompt-aware enqueue helpers.

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::jobs::extract::{get_extract_job, list_extract_jobs, start_extract_job};
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::jobs::WorkerMode;
use crate::crates::services::types::{
    ExecutionMode, ExtractJobResult, ExtractStartResult, JobStartOutcome, StartDisposition,
};
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub use crate::crates::jobs::extract::ExtractJob;

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_extract_start_result(job_id: String) -> ExtractStartResult {
    ExtractStartResult { job_id }
}

pub fn map_extract_job_result(payload: serde_json::Value) -> ExtractJobResult {
    ExtractJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn extract_status(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<ExtractJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(cfg, JobKind::Extract, id).await?;
    Ok(job.map(|value| {
        map_extract_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn extract_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<ExtractJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(cfg, JobKind::Extract, limit, offset).await?;
    Ok(map_extract_job_result(serde_json::to_value(jobs)?))
}

pub async fn extract_cancel(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(cfg, JobKind::Extract, id).await
}

pub async fn extract_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(cfg, JobKind::Extract).await
}

pub async fn extract_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(cfg, JobKind::Extract).await
}

pub async fn extract_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(cfg, JobKind::Extract).await
}

pub async fn extract_status_raw(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<ExtractJob>, Box<dyn Error>> {
    get_extract_job(cfg, id).await
}

pub async fn extract_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<ExtractJob>, Box<dyn Error>> {
    list_extract_jobs(cfg, limit, offset).await
}

pub async fn extract_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match job_service::run_worker(cfg, JobKind::Extract).await? {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
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

    let job_id = start_extract_job(cfg, urls, prompt).await?;

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
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<ExtractStartResult>, Box<dyn Error>> {
    if !cfg.lite_mode {
        let result = extract_start_with_prompt(cfg, urls, prompt, tx).await?;
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::Enqueued,
            result,
        });
    }

    let backend = service_context
        .require_job_backend()
        .map_err(|e| -> Box<dyn Error> { e })?;
    let job_id = backend
        .enqueue(JobPayload::Extract {
            urls: urls.to_vec(),
            config_json: "{}".to_string(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_extract_start_result(job_id.to_string()),
    })
}
