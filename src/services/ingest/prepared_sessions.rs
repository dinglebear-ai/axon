use crate::core::config::Config;
use crate::ingest;
use crate::ingest::progress::PhaseReporter;
use crate::jobs::backend::{JobPayload, JobSidecarPayload};
use crate::jobs::config_snapshot::ingest_config_json;
use crate::jobs::ingest::types::{source_type_label, target_label};
use crate::services::context::ServiceContext;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{
    ExecutionMode, IngestResult, IngestStartResult, JobStartOutcome, StartDisposition,
};
use std::error::Error;
use tokio::sync::mpsc;

use super::{IngestSource, ingest_payload, map_ingest_result, map_ingest_start_result};

pub async fn ingest_sessions_prepared_start_with_context(
    cfg: &Config,
    request: ingest::sessions::IngestSessionsPreparedRequest,
    service_context: &ServiceContext,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    request
        .validate(cfg)
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    let source = IngestSource::PreparedSessions {};
    let config_json = ingest_config_json(cfg, &source)?;
    let payload_json = serde_json::to_string(&request)?;
    let job_id = service_context
        .jobs
        .enqueue_with_sidecar(
            JobPayload::Ingest {
                target: target_label(&source),
                source_type: source_type_label(&source).to_string(),
                config_json,
            },
            JobSidecarPayload::IngestPreparedSessions { payload_json },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(job_id.to_string()),
    })
}

#[must_use = "ingest_sessions_prepared_with_progress returns a Result that should be handled"]
pub async fn ingest_sessions_prepared_with_progress(
    cfg: &Config,
    request: ingest::sessions::IngestSessionsPreparedRequest,
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
    let chunks = ingest::sessions::ingest_prepared_sessions(cfg, request, &reporter)
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
