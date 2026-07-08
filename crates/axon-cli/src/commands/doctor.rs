pub(crate) mod render;

use axon_core::config::Config;
use axon_services::context::ServiceContext;
use render::render_doctor_report_human;
use std::error::Error;

pub async fn run_doctor(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let result = axon_services::system::doctor(service_context)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    let report = result.payload;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        render_doctor_report_human(&report);
        if cfg.doctor_diagnose {
            println!(
                "LLM diagnosis unavailable: configure AXON_HEADLESS_GEMINI_CMD to enable doctor diagnose."
            );
        }
    }
    Ok(())
}
