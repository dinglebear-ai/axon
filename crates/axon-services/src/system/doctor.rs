//! Service connectivity diagnostics — Qdrant / TEI / LLM reachability.

use crate::types::DoctorResult;
use axon_core::config::Config;
use axon_core::health::build_doctor_report;
use std::error::Error;

pub fn map_doctor_payload(payload: serde_json::Value) -> DoctorResult {
    DoctorResult { payload }
}

#[must_use = "doctor returns a Result that should be handled"]
pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>> {
    let pending_jobs = axon_jobs::store::count_pending_jobs(&cfg.sqlite_path).await;
    let llm_probe = axon_llm::build_llm_doctor_probe(cfg).await;
    let payload = build_doctor_report(cfg, pending_jobs, llm_probe)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("doctor health check failed: {e}").into() })?;
    Ok(map_doctor_payload(payload))
}
