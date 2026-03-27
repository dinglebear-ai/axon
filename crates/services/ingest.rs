use crate::crates::core::config::Config;
use crate::crates::ingest;
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::jobs::backend::{JobKind, JobPayload};
pub use crate::crates::jobs::ingest::{IngestJob, IngestSource};
use crate::crates::jobs::ingest::{get_ingest_job, list_ingest_jobs, start_ingest_job};
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobStartOutcome,
    StartDisposition,
};
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod classify;
pub use classify::classify_target;

// --- Pure mapping helper (no I/O, testable without live services) ---

pub fn map_ingest_result(payload: serde_json::Value) -> IngestResult {
    IngestResult { payload }
}

pub fn map_ingest_start_result(job_id: String) -> IngestStartResult {
    IngestStartResult { job_id }
}

pub fn map_ingest_job_result(payload: serde_json::Value) -> IngestJobResult {
    IngestJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn ingest_start(
    cfg: &Config,
    source: IngestSource,
) -> Result<IngestStartResult, Box<dyn Error>> {
    let job_id = start_ingest_job(cfg, source).await?;
    Ok(map_ingest_start_result(job_id.to_string()))
}

pub async fn ingest_start_with_context(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    if !cfg.lite_mode {
        let result = ingest_start(cfg, source).await?;
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::Enqueued,
            result,
        });
    }

    let (source_type, target) = match &source {
        IngestSource::Github { repo, .. } => ("github".to_string(), repo.clone()),
        IngestSource::Reddit { target } => ("reddit".to_string(), target.clone()),
        IngestSource::Youtube { target } => ("youtube".to_string(), target.clone()),
        IngestSource::Sessions { .. } => {
            return Err(anyhow::anyhow!(
                "sessions ingest is handled by the sessions command, not ingest"
            )
            .into());
        }
    };
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Ingest {
            target,
            source_type,
            config_json: "{}".to_string(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(job_id.to_string()),
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
) -> Result<Vec<IngestJob>, Box<dyn Error>> {
    list_ingest_jobs(cfg, None, limit, offset).await
}

pub async fn ingest_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::run_worker(service_context, JobKind::Ingest).await? {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

/// Ingest a GitHub repository (code, issues, PRs, wiki) into the vector store.
///
/// Calls `ingest::github::ingest_github` which performs the fetch and embed
/// synchronously. For async/fire-and-forget behaviour use the job queue via
/// the ingest CLI command.
pub async fn ingest_github(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    let repo_slug = format!("{owner}/{repo}");

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting github repo: {repo_slug}"),
        },
    )
    .await;

    let chunks = ingest::github::ingest_github(
        cfg,
        &repo_slug,
        cfg.github_include_source,
        PhaseReporter::noop(),
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        format!("github ingest failed for {repo_slug}: {e}").into()
    })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("github ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "github",
        "repo": repo_slug,
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

/// Ingest a Reddit subreddit or thread into the vector store.
///
/// `target` may be a subreddit name (e.g. `"rust"`) or a full thread URL.
pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting reddit target: {target}"),
        },
    )
    .await;

    let noop = PhaseReporter::noop();
    let chunks = ingest::reddit::ingest_reddit(cfg, target, &noop)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("reddit ingest failed for {target}: {e}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("reddit ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "reddit",
        "target": target,
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

/// Ingest YouTube content into the vector store.
///
/// `url` may be a single video URL, a bare video ID, a playlist URL
/// (`youtube.com/playlist?list=...`), or a channel URL (`/@handle`, `/c/`, `/channel/`).
pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting youtube: {url}"),
        },
    )
    .await;

    let noop = PhaseReporter::noop();
    let chunks = ingest::youtube::ingest_youtube(cfg, url, &noop)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("youtube ingest failed for {url}: {e}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("youtube ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "youtube",
        "url": url,
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

/// Ingest AI session exports (Claude/Codex/Gemini) into the vector store.
///
/// Session sources and paths are read from cfg (sessions_claude, sessions_codex,
/// sessions_gemini, sessions_project).
pub async fn ingest_sessions(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ingesting session exports".to_string(),
        },
    )
    .await;

    let noop = PhaseReporter::noop();
    let chunks = ingest::sessions::ingest_sessions(cfg, &noop)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("session exports ingest failed: {e}").into() })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("sessions ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "sessions",
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}
