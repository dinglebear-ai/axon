use crate::context::ServiceContext;
use crate::types::{ExecutionMode, IngestStartResult, JobStartOutcome, StartDisposition};
use axon_core::config::Config;
use axon_ingest as ingest;
use axon_jobs::backend::{JobPayload, JobSidecarPayload};
use axon_jobs::config_snapshot::ingest_config_json;
use axon_jobs::ingest::types::{source_type_label, target_label};
use std::error::Error;

use super::{IngestSource, map_ingest_start_result};

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
