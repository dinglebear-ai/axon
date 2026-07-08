//! Source-ingestion orchestration: drives session-export ingest and reports
//! progress over an optional `ServiceEvent` channel, returning an `IngestResult`.
//!
//! These functions take only `cfg` + progress channels (no `ServiceContext`,
//! no jobs), so they live here in `axon-ingest` and are called by both the
//! services layer and the jobs ingest runner.
//!
//! Phase 12 clean break (issue #298): the github/gitlab/gitea/generic_git/
//! reddit/youtube/rss provider orchestration that used to live here was
//! deleted outright — only session-export ingest is still executed by the
//! legacy per-family job runner. `classify_target`'s IngestSource variants
//! for those providers remain (backed by `crate::target_parse`) since
//! `axon refresh` still needs to classify previously-ingested origins.

use axon_api::job_dto::IngestResult;
use axon_core::config::Config;
use axon_core::events::{LogLevel, ServiceEvent, emit};
use std::error::Error;
use tokio::sync::mpsc;

use crate::progress::PhaseReporter;

mod sessions_prepared;

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
