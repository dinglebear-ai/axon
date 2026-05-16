//! Service connectivity diagnostics — Qdrant / TEI / LLM reachability.

use crate::core::config::Config;
use crate::core::health::build_doctor_report;
use crate::services::types::DoctorResult;
use std::error::Error;

pub fn map_doctor_payload(payload: serde_json::Value) -> DoctorResult {
    DoctorResult { payload }
}

#[must_use = "doctor returns a Result that should be handled"]
pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>> {
    let payload = build_doctor_report(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("doctor health check failed: {e}").into() })?;
    Ok(map_doctor_payload(payload))
}
