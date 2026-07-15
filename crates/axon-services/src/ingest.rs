use crate::context::ServiceContext;
use crate::jobs as job_service;
use crate::runtime::WorkerMode;
use crate::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobListResult,
    JobStartOutcome, StartDisposition,
};
use axon_api::source::{AuthSnapshot, JobKind, JobPriority, SourceIntent, SourceRequest};
use axon_core::config::Config;
pub use axon_jobs::ingest::IngestSource;
use axon_jobs::ingest::types::{source_type_label, target_label};
use std::error::Error;
use uuid::Uuid;

pub mod classify;
mod classify_target;
pub(crate) mod orchestrate;
pub mod request;
mod target_parse;
pub use classify::classify_target;
pub use orchestrate::{ingest_payload, map_ingest_result};
pub use request::{source_from_mcp_request, validate_ingest_source};

pub fn map_ingest_start_result(job_id: String) -> IngestStartResult {
    IngestStartResult { job_id }
}

pub fn map_ingest_job_result(payload: serde_json::Value) -> IngestJobResult {
    IngestJobResult { payload }
}

// --- Service lifecycle wrappers ---

/// Pre-flight existence check run before an ingest source request is enqueued.
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
    let request = ingest_source_request(cfg, &source)?;
    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;
    let auth_snapshot = caller
        .cloned()
        .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime"));
    let source_result =
        crate::source::enqueue::enqueue_source(request, store.as_ref(), Some(auth_snapshot))
            .await
            .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    let descriptor = source_result.job.ok_or_else(|| -> Box<dyn Error> {
        source_result
            .errors
            .first()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| "failed to enqueue source-backed ingest job".to_string())
            .into()
    })?;
    service_context.notify_unified();
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(descriptor.id.0.to_string()),
    })
}

fn ingest_source_request(
    cfg: &Config,
    source: &IngestSource,
) -> Result<SourceRequest, Box<dyn Error>> {
    let source_ref = match source {
        IngestSource::Github { repo, .. } => repo.clone(),
        IngestSource::Gitlab { target, .. }
        | IngestSource::Gitea { target, .. }
        | IngestSource::GenericGit { target, .. }
        | IngestSource::Youtube { target }
        | IngestSource::Rss { target } => target.clone(),
        IngestSource::Reddit { target } => {
            if target.starts_with("r/")
                || target.starts_with("reddit.com/")
                || target.starts_with("https://reddit.com/")
                || target.starts_with("https://www.reddit.com/")
            {
                target.clone()
            } else {
                format!("r/{target}")
            }
        }
        IngestSource::Sessions { .. } => {
            return Err("sessions ingest must use the source session selector path".into());
        }
    };
    let mut request = SourceRequest::new(source_ref);
    request.intent = SourceIntent::Acquire;
    request.embed = true;
    request.collection = Some(cfg.collection.clone());
    request.execution.priority = JobPriority::Normal;
    request.options.values.insert(
        "ingest_source_type".to_string(),
        serde_json::json!(source_type_label(source)),
    );
    request.options.values.insert(
        "ingest_target".to_string(),
        serde_json::json!(target_label(source)),
    );
    Ok(request)
}

pub async fn ingest_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Source, id).await?;
    Ok(job.map(|value| {
        map_ingest_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn ingest_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<IngestResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Source, limit, offset).await?;
    Ok(map_ingest_result(serde_json::to_value(jobs)?))
}

pub async fn ingest_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Source, id).await
}

pub async fn ingest_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Source).await
}

pub async fn ingest_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Source).await
}

pub async fn ingest_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Source).await
}

pub async fn ingest_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Source).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}
