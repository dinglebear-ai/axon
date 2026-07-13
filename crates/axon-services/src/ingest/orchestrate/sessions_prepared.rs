use axon_api::job_dto::IngestResult;
use axon_core::config::Config;
use axon_core::events::{LogLevel, ServiceEvent, emit};
use std::error::Error;
use tokio::sync::mpsc;

use super::{ingest_payload, map_ingest_result};
use crate::ingest::progress::PhaseReporter;

#[must_use = "ingest_sessions_prepared_with_progress returns a Result that should be handled"]
pub async fn ingest_sessions_prepared_with_progress(
    cfg: &Config,
    request: crate::sessions_legacy::IngestSessionsPreparedRequest,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ingesting prepared session exports".to_string(),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = crate::sessions_legacy::ingest_prepared_sessions(cfg, request, &reporter)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("prepared session exports ingest failed: {e}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("prepared sessions ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = ingest_payload("prepared_sessions", None, chunks);
    Ok(map_ingest_result(payload))
}
