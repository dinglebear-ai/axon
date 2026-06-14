use crate::core::config::Config;
use crate::ingest;
use crate::ingest::progress::PhaseReporter;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::config_snapshot::ingest_config_json;
use crate::jobs::ingest::types::{source_type_label, target_label};
pub use crate::jobs::ingest::{IngestJob, IngestSource};
use crate::jobs::ingest::{count_ingest_jobs, get_ingest_job, list_ingest_jobs, start_ingest_job};
use crate::services::context::ServiceContext;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::jobs as job_service;
use crate::services::runtime::WorkerMode;
use crate::services::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobListResult,
    JobStartOutcome, StartDisposition,
};
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod classify;
pub mod git_services;
mod prepared_sessions;
pub mod request;
mod rss;
pub use classify::classify_target;
pub use git_services::{ingest_generic_git_with_progress, ingest_gitea_with_progress};
pub use prepared_sessions::{
    ingest_sessions_prepared_start_with_context, ingest_sessions_prepared_with_progress,
};
pub use request::{source_from_mcp_request, validate_ingest_source};
pub use rss::{ingest_rss, ingest_rss_with_progress};

// --- Pure mapping helper (no I/O, testable without live services) ---

pub fn map_ingest_result(payload: serde_json::Value) -> IngestResult {
    IngestResult { payload }
}

pub(crate) fn ingest_payload(
    source: &str,
    target_field: Option<(&str, &str)>,
    chunks_embedded: usize,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "source": source,
        "chunks_embedded": chunks_embedded,
        // Preserve the legacy key for existing API and CLI callers while making
        // `chunks_embedded` the canonical progress/result field.
        "chunks": chunks_embedded,
    });
    if let Some((key, value)) = target_field
        && let Some(object) = payload.as_object_mut()
    {
        object.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    payload
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
    // Always route through service_context.jobs.enqueue() so that notify()
    // fires immediately and workers wake without 0-5 second polling delay.
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    let config_json = ingest_config_json(cfg, &source)?;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Ingest {
            target,
            source_type,
            config_json,
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

// --- Service functions ---

/// Ingest a GitHub repository (code, issues, PRs, wiki) into the vector store.
///
/// Calls `ingest::github::ingest_github` which performs the fetch and embed
/// synchronously. For async/fire-and-forget behaviour use the job queue via
/// the ingest CLI command.
#[must_use = "ingest_github returns a Result that should be handled"]
pub async fn ingest_github(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_github_with_progress(cfg, owner, repo, tx, None).await
}

/// Ingest a GitHub repository with an optional structured progress sink.
#[must_use = "ingest_github_with_progress returns a Result that should be handled"]
pub async fn ingest_github_with_progress(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
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
        PhaseReporter::new(progress_tx),
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        format!("github ingest failed for {repo_slug}: {e:#}").into()
    })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("github ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = ingest_payload("github", Some(("repo", &repo_slug)), chunks);
    Ok(map_ingest_result(payload))
}

/// Ingest a GitLab project with an optional structured progress sink.
#[must_use = "ingest_gitlab_with_progress returns a Result that should be handled"]
pub async fn ingest_gitlab_with_progress(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting gitlab project: {target}"),
        },
    )
    .await;

    let chunks = ingest::gitlab::ingest_gitlab(
        cfg,
        target,
        cfg.github_include_source,
        PhaseReporter::new(progress_tx),
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        format!("gitlab ingest failed for {target}: {e:#}").into()
    })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("gitlab ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = ingest_payload("gitlab", Some(("target", target)), chunks);
    Ok(map_ingest_result(payload))
}

/// Ingest a Reddit subreddit or thread into the vector store.
///
/// `target` may be a subreddit name (e.g. `"rust"`) or a full thread URL.
#[must_use = "ingest_reddit returns a Result that should be handled"]
pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_reddit_with_progress(cfg, target, tx, None).await
}

/// Ingest a Reddit subreddit or thread with an optional structured progress sink.
#[must_use = "ingest_reddit_with_progress returns a Result that should be handled"]
pub async fn ingest_reddit_with_progress(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_reddit_with_progress_and_options(
        cfg,
        target,
        tx,
        progress_tx,
        &ingest::reddit::RedditIngestOptions::default(),
    )
    .await
}

/// Ingest a Reddit subreddit or thread with progress and source-local controls.
#[must_use = "ingest_reddit_with_progress_and_options returns a Result that should be handled"]
pub async fn ingest_reddit_with_progress_and_options(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
    options: &ingest::reddit::RedditIngestOptions,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting reddit target: {target}"),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let summary = ingest::reddit::ingest_reddit_with_options(cfg, target, &reporter, options)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("reddit ingest failed for {target}: {e:#}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("reddit ingest complete: {} chunks", summary.chunks_embedded),
        },
    )
    .await;

    let mut payload = ingest_payload("reddit", Some(("target", target)), summary.chunks_embedded);
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "reddit_stats".to_string(),
            serde_json::json!({
                "posts_seen": summary.stats.posts_seen,
                "posts_prepared": summary.stats.posts_prepared,
                "comment_fetch_attempts": summary.stats.comment_fetch_attempts,
                "comment_fetch_failures": summary.stats.comment_fetch_failures,
                "partial_comment_failures": summary.stats.has_partial_comment_failures(),
            }),
        );
    }
    Ok(map_ingest_result(payload))
}

/// Ingest YouTube content into the vector store.
///
/// `url` may be a single video URL, a bare video ID, a playlist URL
/// (`youtube.com/playlist?list=...`), or a channel URL (`/@handle`, `/c/`, `/channel/`).
#[must_use = "ingest_youtube returns a Result that should be handled"]
pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_youtube_with_progress(cfg, url, tx, None).await
}

/// Ingest YouTube content with an optional structured progress sink.
#[must_use = "ingest_youtube_with_progress returns a Result that should be handled"]
pub async fn ingest_youtube_with_progress(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting youtube: {url}"),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = ingest::youtube::ingest_youtube_target(cfg, url, &reporter)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("youtube ingest failed for {url}: {e:#}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("youtube ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = ingest_payload("youtube", Some(("url", url)), chunks);
    Ok(map_ingest_result(payload))
}

/// Ingest AI session exports (Claude/Codex/Gemini) into the vector store.
///
/// Session sources and paths are read from cfg (sessions_claude, sessions_codex,
/// sessions_gemini, sessions_project).
#[must_use = "ingest_sessions returns a Result that should be handled"]
pub async fn ingest_sessions(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_sessions_with_progress(cfg, tx, None).await
}

/// Ingest AI session exports with an optional structured progress sink.
#[must_use = "ingest_sessions_with_progress returns a Result that should be handled"]
pub async fn ingest_sessions_with_progress(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ingesting session exports".to_string(),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = ingest::sessions::ingest_sessions(cfg, &reporter)
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

    let payload = ingest_payload("sessions", None, chunks);
    Ok(map_ingest_result(payload))
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
