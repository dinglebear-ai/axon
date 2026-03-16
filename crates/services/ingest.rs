use crate::crates::core::config::Config;
use crate::crates::ingest;
pub use crate::crates::jobs::ingest::{IngestJob, IngestSource};
use crate::crates::jobs::ingest::{
    cancel_ingest_job, cleanup_ingest_jobs, clear_ingest_jobs, get_ingest_job, list_ingest_jobs,
    recover_stale_ingest_jobs, run_ingest_worker, start_ingest_job,
};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{IngestJobResult, IngestResult, IngestStartResult};
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

pub async fn ingest_status(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = get_ingest_job(cfg, id).await?;
    Ok(job.map(|value| {
        map_ingest_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn ingest_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<IngestResult, Box<dyn Error>> {
    let jobs = list_ingest_jobs(cfg, None, limit, offset).await?;
    Ok(map_ingest_result(serde_json::to_value(jobs)?))
}

pub async fn ingest_cancel(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    cancel_ingest_job(cfg, id).await
}

pub async fn ingest_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    cleanup_ingest_jobs(cfg).await
}

pub async fn ingest_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    clear_ingest_jobs(cfg).await
}

pub async fn ingest_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    recover_stale_ingest_jobs(cfg).await
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

pub async fn ingest_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    run_ingest_worker(cfg).await
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

    let chunks =
        ingest::github::ingest_github(cfg, &repo_slug, cfg.github_include_source, None).await?;

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

    let chunks = ingest::reddit::ingest_reddit(cfg, target).await?;

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

    let chunks = ingest::youtube::ingest_youtube(cfg, url).await?;

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

    let chunks = ingest::sessions::ingest_sessions(cfg).await?;

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
