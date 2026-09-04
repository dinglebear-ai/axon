//! Service connectivity diagnostics — Qdrant / TEI / LLM reachability.
//!
//! `doctor` is a `provider_probe` job-backed operation (see
//! `docs/pipeline-unification/runtime/job-contract.md`, job kind
//! `provider_probe`: "Health/capability check"). Every call creates a
//! unified job row via `super::job_tracking::track_operation_job`, tracking
//! the health-check run through the standard Queued -> Running ->
//! Completed/Failed lifecycle.
//!
//! Returns `Box<dyn Error + Send + Sync>` (not the bare `Box<dyn Error>` used
//! by sibling `system` functions) because the MCP `#[tool]`-macro-generated
//! `axon` handler requires the whole call chain's future to be `Send` — see
//! `super::job_tracking` for the full explanation. `Box<dyn Error + Send +
//! Sync>` still satisfies callers that only need `Box<dyn Error>` via the
//! blanket `From` conversion.

use crate::context::ServiceContext;
use crate::types::DoctorResult;
use axon_api::source::OperationKind;
use axon_core::config::Config;
use axon_core::health::build_doctor_report;
use serde_json::json;
use std::error::Error;

pub fn map_doctor_payload(payload: serde_json::Value) -> DoctorResult {
    DoctorResult { payload }
}

#[must_use = "doctor returns a Result that should be handled"]
pub async fn doctor(ctx: &ServiceContext) -> Result<DoctorResult, Box<dyn Error + Send + Sync>> {
    let request_json = json!({ "operation": "provider_probe" });
    super::job_tracking::track_operation_job(
        ctx,
        OperationKind::ProviderProbe,
        request_json,
        || doctor_inner(ctx.cfg.as_ref()),
    )
    .await
}

/// The real connectivity checks, without the job-tracking wrapper. Exposed
/// `pub(crate)` so `runtime::job_runners::ProviderProbeRunner` (which runs
/// *inside* an already-tracked unified `provider_probe` job) can call it
/// directly instead of double-tracking a job for the same probe.
pub(crate) async fn doctor_inner(
    cfg: &Config,
) -> Result<DoctorResult, Box<dyn Error + Send + Sync>> {
    let pending_jobs = axon_jobs::store::count_pending_jobs(&cfg.sqlite_path).await;
    let llm_probe = axon_llm::build_llm_doctor_probe(cfg).await;
    let payload = build_doctor_report(cfg, pending_jobs, llm_probe)
        .await
        .map_err(|e| -> Box<dyn Error + Send + Sync> {
            format!("doctor health check failed: {e}").into()
        })?;
    Ok(map_doctor_payload(payload))
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
