//! Source-ingestion orchestration: drives each provider's ingest and reports
//! progress over an optional `ServiceEvent` channel, returning an `IngestResult`.
//!
//! These functions take only `cfg` + progress channels (no `ServiceContext`,
//! no jobs), so they live here in `axon-ingest` and are called by both the
//! services layer and the jobs ingest runner.

use axon_api::job_dto::IngestResult;
use axon_core::config::Config;
use axon_core::events::{LogLevel, ServiceEvent, emit};
use std::error::Error;
use tokio::sync::mpsc;

use crate::progress::PhaseReporter;

mod git;
mod rss;
mod sessions_prepared;

pub use git::{ingest_generic_git_with_progress, ingest_gitea_with_progress};
pub use rss::{ingest_rss, ingest_rss_with_progress};
pub use sessions_prepared::ingest_sessions_prepared_with_progress;

pub fn map_ingest_result(payload: serde_json::Value) -> IngestResult {
    IngestResult { payload }
}

pub fn ingest_payload(
    source: &str,
    target_field: Option<(&str, &str)>,
    chunks_embedded: usize,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "source": source,
        "chunks_embedded": chunks_embedded,
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

#[must_use = "ingest_github returns a Result that should be handled"]
pub async fn ingest_github(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_github_with_progress(cfg, owner, repo, tx, None).await
}

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
    let chunks = crate::github::ingest_github(
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
    Ok(map_ingest_result(ingest_payload(
        "github",
        Some(("repo", &repo_slug)),
        chunks,
    )))
}

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
    let chunks = crate::gitlab::ingest_gitlab(
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
    Ok(map_ingest_result(ingest_payload(
        "gitlab",
        Some(("target", target)),
        chunks,
    )))
}

#[must_use = "ingest_reddit returns a Result that should be handled"]
pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_reddit_with_progress(cfg, target, tx, None).await
}

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
        &crate::reddit::RedditIngestOptions::default(),
    )
    .await
}

#[must_use = "ingest_reddit_with_progress_and_options returns a Result that should be handled"]
pub async fn ingest_reddit_with_progress_and_options(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
    options: &crate::reddit::RedditIngestOptions,
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
    let summary = crate::reddit::ingest_reddit_with_options(cfg, target, &reporter, options)
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

#[must_use = "ingest_youtube returns a Result that should be handled"]
pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_youtube_with_progress(cfg, url, tx, None).await
}

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
    let chunks = crate::youtube::ingest_youtube_target(cfg, url, &reporter)
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
    Ok(map_ingest_result(ingest_payload(
        "youtube",
        Some(("url", url)),
        chunks,
    )))
}

#[must_use = "ingest_sessions returns a Result that should be handled"]
pub async fn ingest_sessions(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_sessions_with_progress(cfg, tx, None).await
}

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
    let chunks = crate::sessions::ingest_sessions(cfg, &reporter)
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
    Ok(map_ingest_result(ingest_payload("sessions", None, chunks)))
}
