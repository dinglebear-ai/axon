use std::error::Error;

use tokio::sync::mpsc;

use crate::progress::PhaseReporter;
use axon_api::job_dto::IngestResult;
use axon_core::config::Config;
use axon_core::events::{LogLevel, ServiceEvent, emit};

use super::{ingest_payload, map_ingest_result};

#[must_use = "ingest_gitea_with_progress returns a Result that should be handled"]
pub async fn ingest_gitea_with_progress(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting gitea repo: {target}"),
        },
    )
    .await;
    let chunks = crate::gitea::ingest_gitea(
        cfg,
        target,
        cfg.github_include_source,
        PhaseReporter::new(progress_tx),
    )
    .await
    .map_err(|e| -> Box<dyn Error> { format!("gitea ingest failed for {target}: {e:#}").into() })?;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("gitea ingest complete: {chunks} chunks"),
        },
    )
    .await;
    Ok(map_ingest_result(ingest_payload(
        "gitea",
        Some(("target", target)),
        chunks,
    )))
}

#[must_use = "ingest_generic_git_with_progress returns a Result that should be handled"]
pub async fn ingest_generic_git_with_progress(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting git repo: {target}"),
        },
    )
    .await;
    let chunks = crate::generic_git::ingest_generic_git(
        cfg,
        target,
        cfg.github_include_source,
        PhaseReporter::new(progress_tx),
    )
    .await
    .map_err(|e| -> Box<dyn Error> { format!("git ingest failed for {target}: {e:#}").into() })?;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("git ingest complete: {chunks} chunks"),
        },
    )
    .await;
    Ok(map_ingest_result(ingest_payload(
        "git",
        Some(("target", target)),
        chunks,
    )))
}
