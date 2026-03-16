//! Service-layer wrappers for embed job lifecycle operations and synchronous embedding entry points.

use crate::crates::core::config::Config;
use crate::crates::jobs::embed::{
    self as embed_jobs, cancel_embed_job, cleanup_embed_jobs, clear_embed_jobs, get_embed_job,
    list_embed_jobs, recover_stale_embed_jobs, start_embed_job,
};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{EmbedJobResult, EmbedStartResult};
use crate::crates::vector::ops::embed_path_native;
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub use crate::crates::jobs::embed::EmbedJob;

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_embed_start_result(job_id: String) -> EmbedStartResult {
    EmbedStartResult { job_id }
}

pub fn map_embed_job_result(payload: serde_json::Value) -> EmbedJobResult {
    EmbedJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn embed_status(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<EmbedJobResult>, Box<dyn Error>> {
    let job = get_embed_job(cfg, id).await?;
    Ok(job.map(|value| {
        map_embed_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn embed_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    let jobs = list_embed_jobs(cfg, limit, offset).await?;
    Ok(map_embed_job_result(serde_json::to_value(jobs)?))
}

pub async fn embed_cancel(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    cancel_embed_job(cfg, id).await
}

pub async fn embed_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    cleanup_embed_jobs(cfg).await
}

pub async fn embed_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    clear_embed_jobs(cfg).await
}

pub async fn embed_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    recover_stale_embed_jobs(cfg).await
}

pub async fn embed_status_raw(cfg: &Config, id: Uuid) -> Result<Option<EmbedJob>, Box<dyn Error>> {
    get_embed_job(cfg, id).await
}

pub async fn embed_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<EmbedJob>, Box<dyn Error>> {
    list_embed_jobs(cfg, limit, offset).await
}

pub async fn embed_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    embed_jobs::run_embed_worker(cfg)
        .await
        .map_err(|err| -> Box<dyn Error> { err.into() })
}

// --- Service functions ---

pub async fn embed_start_with_input(
    cfg: &Config,
    input: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<EmbedStartResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing embed job for input: {input}"),
        },
    )
    .await;

    let job_id = start_embed_job(cfg, input).await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueued embed job: {job_id}"),
        },
    )
    .await;

    Ok(map_embed_start_result(job_id.to_string()))
}

/// Enqueue an embed job for the input specified in cfg and return its job ID
/// immediately. The embed input is resolved from cfg.positional or cfg.output_dir
/// following the same logic as the CLI embed command.
pub async fn embed_start(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<EmbedStartResult, Box<dyn Error>> {
    let input = cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    });

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing embed job for input: {input}"),
        },
    )
    .await;

    let job_id = start_embed_job(cfg, &input).await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueued embed job: {job_id}"),
        },
    )
    .await;

    Ok(map_embed_start_result(job_id.to_string()))
}

pub async fn embed_now(cfg: &Config, input: &str) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native(cfg, input).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}
