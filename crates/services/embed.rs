//! Service-layer wrappers for embed job lifecycle operations and synchronous embedding entry points.

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::jobs::embed::{get_embed_job, list_embed_jobs, start_embed_job};
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::ServiceJobRuntime;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    EmbedJobResult, EmbedStartResult, ExecutionMode, JobStartOutcome, StartDisposition,
};
use crate::crates::vector::ops::{embed_path_native, embed_path_native_with_progress};
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
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<EmbedJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    Ok(job.map(|value| {
        map_embed_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn embed_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Embed, limit, offset).await?;
    Ok(map_embed_job_result(serde_json::to_value(jobs)?))
}

pub async fn embed_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Embed, id).await
}

pub async fn embed_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Embed).await
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

pub async fn embed_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::run_worker(service_context, JobKind::Embed).await? {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

pub async fn embed_start_with_input(
    cfg: &Config,
    input: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    source_type: Option<&str>,
) -> Result<EmbedStartResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing embed job for input: {input}"),
        },
    )
    .await;

    let job_id = start_embed_job(cfg, input, source_type).await?;

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

pub async fn embed_start_with_context(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    source_type: Option<&str>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    if !cfg.lite_mode {
        let result = embed_start_with_input(cfg, input, tx, source_type).await?;
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::Enqueued,
            result,
        });
    }

    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Embed {
            input: input.to_string(),
            config_json: "{}".to_string(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    wait_for_embed_completion(service_context.jobs.as_ref(), job_id).await?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Completed,
        execution_mode: ExecutionMode::InProcess,
        result: map_embed_start_result(job_id.to_string()),
    })
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
    embed_start_with_input(cfg, &input, tx, None).await
}

pub async fn embed_now(cfg: &Config, input: &str) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native(cfg, input).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

pub async fn embed_now_with_source(
    cfg: &Config,
    input: &str,
    source_type: Option<&str>,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None, source_type).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

async fn wait_for_embed_completion(
    runtime: &dyn ServiceJobRuntime,
    job_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let final_status = runtime
        .wait_for_job(job_id, JobKind::Embed)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    if final_status == "failed" {
        if let Ok(Some(err)) = runtime.job_errors(job_id, JobKind::Embed).await {
            return Err(format!("embed job {job_id} failed: {err}").into());
        }
        return Err(format!("embed job {job_id} failed").into());
    }
    Ok(())
}
