//! Service-layer wrappers for extract job lifecycle operations and prompt-aware enqueue helpers.

use crate::crates::core::config::Config;
use crate::crates::jobs::extract::{
    self as extract_jobs, cancel_extract_job, cleanup_extract_jobs, clear_extract_jobs,
    get_extract_job, list_extract_jobs, recover_stale_extract_jobs, start_extract_job,
};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{ExtractJobResult, ExtractStartResult};
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
    let job = get_extract_job(cfg, id).await?;
    Ok(job.map(|value| {
        map_extract_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn extract_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<ExtractJobResult, Box<dyn Error>> {
    let jobs = list_extract_jobs(cfg, limit, offset).await?;
    Ok(map_extract_job_result(serde_json::to_value(jobs)?))
}

pub async fn extract_cancel(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    cancel_extract_job(cfg, id).await
}

pub async fn extract_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    cleanup_extract_jobs(cfg).await
}

pub async fn extract_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    clear_extract_jobs(cfg).await
}

pub async fn extract_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    recover_stale_extract_jobs(cfg).await
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
    extract_jobs::run_extract_worker(cfg).await
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
