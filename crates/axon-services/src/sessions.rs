use crate::context::ServiceContext;
use crate::sessions_legacy::watch::{
    NoopSessionWatchEventSink, SessionWatchEventSink, SessionWatchIngestor, WatchIngestResult,
    run_session_watch, smoke_watch,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use axon_core::config::{Config, SessionWatchConfig};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionWatchError {
    pub path_hash: String,
    pub provider: String,
    pub basename: String,
    pub error_code: String,
    pub error_redacted: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionWatchStatus {
    pub checkpoint_count: i64,
    pub error_count: i64,
    pub recent_errors: Vec<SessionWatchError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionWatchSmokeReport {
    pub transcript_path: PathBuf,
    pub probe_text: String,
    pub ingested: bool,
    pub evidence: String,
}

pub async fn run_watch(
    cfg: &Config,
    service_context: &ServiceContext,
    options: SessionWatchConfig,
) -> Result<()> {
    run_watch_with_event_sink(cfg, service_context, options, &NoopSessionWatchEventSink).await
}

pub async fn run_watch_with_event_sink(
    cfg: &Config,
    service_context: &ServiceContext,
    options: SessionWatchConfig,
    events: &dyn SessionWatchEventSink,
) -> Result<()> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or_else(|| anyhow!("session watch requires the SQLite job runtime"))?;
    let ingestor = ServiceSessionWatchIngestor;
    run_session_watch(cfg, pool.as_ref(), &ingestor, options, events).await
}

pub async fn watch_status(
    service_context: &ServiceContext,
    limit: usize,
) -> Result<SessionWatchStatus> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or_else(|| anyhow!("watch-status requires the SQLite job runtime"))?;
    crate::sessions_legacy::checkpoint::watch_status(pool.as_ref(), limit as i64)
        .await
        .map(SessionWatchStatus::from)
}

pub async fn smoke(
    cfg: &Config,
    service_context: &ServiceContext,
    timeout_secs: u64,
) -> Result<SessionWatchSmokeReport> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or_else(|| anyhow!("smoke-watch requires the SQLite job runtime"))?;
    smoke_watch(cfg, pool.as_ref(), timeout_secs)
        .await
        .map(SessionWatchSmokeReport::from)
}

struct ServiceSessionWatchIngestor;

#[async_trait]
impl SessionWatchIngestor for ServiceSessionWatchIngestor {
    async fn ingest_prepared_request_for_watch(
        &self,
        cfg: &Config,
        request: crate::sessions_legacy::IngestSessionsPreparedRequest,
    ) -> Result<WatchIngestResult> {
        let outcome =
            crate::ingest::ingest_sessions_prepared_with_progress(cfg, request, None, None)
                .await
                .map_err(|err| anyhow!(err.to_string()))?;
        let chunks = outcome
            .payload
            .get("chunks")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        Ok(WatchIngestResult::Completed(format!(
            "prepared-session-chunks={chunks}"
        )))
    }
}

impl From<crate::sessions_legacy::checkpoint::SessionWatchStatus> for SessionWatchStatus {
    fn from(status: crate::sessions_legacy::checkpoint::SessionWatchStatus) -> Self {
        Self {
            checkpoint_count: status.checkpoint_count,
            error_count: status.error_count,
            recent_errors: status
                .recent_errors
                .into_iter()
                .map(SessionWatchError::from)
                .collect(),
        }
    }
}

impl From<crate::sessions_legacy::checkpoint::SessionWatchError> for SessionWatchError {
    fn from(error: crate::sessions_legacy::checkpoint::SessionWatchError) -> Self {
        Self {
            path_hash: error.path_hash,
            provider: error.provider,
            basename: error.basename,
            error_code: error.error_code,
            error_redacted: error.error_redacted,
            occurred_at: error.occurred_at,
        }
    }
}

impl From<crate::sessions_legacy::watch::SessionWatchSmokeReport> for SessionWatchSmokeReport {
    fn from(report: crate::sessions_legacy::watch::SessionWatchSmokeReport) -> Self {
        Self {
            transcript_path: report.transcript_path,
            probe_text: report.probe_text,
            ingested: report.ingested,
            evidence: report.evidence,
        }
    }
}
