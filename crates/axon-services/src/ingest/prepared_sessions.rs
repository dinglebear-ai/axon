use crate::context::ServiceContext;
use crate::types::{ExecutionMode, IngestStartResult, JobStartOutcome, StartDisposition};
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobIntent, JobKind, JobPriority, JobStagePlan, MetadataMap,
    PipelinePhase,
};
use axon_core::config::Config;
use std::error::Error;

use super::map_ingest_start_result;

pub async fn ingest_sessions_prepared_start_with_context(
    cfg: &Config,
    request: crate::sessions_legacy::IngestSessionsPreparedRequest,
    service_context: &ServiceContext,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    request
        .validate(cfg)
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    let store = service_context
        .job_store()
        .ok_or("unified job store is not available for this runtime")?;
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Acquire,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![JobStagePlan {
                phase: PipelinePhase::Parsing,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: Some(request.docs.len() as u64),
            }],
            request: Some(serde_json::json!({ "prepared_sessions": request })),
            auth_snapshot: AuthSnapshot::trusted_system("runtime"),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .map_err(|error| -> Box<dyn Error> { error.message.into() })?;
    service_context.notify_unified();
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(descriptor.id.0.to_string()),
    })
}
