use crate::commands::doctor::render::{report_bool, report_text};
use axon_core::config::Config;
use axon_core::redact::redact_secrets;
use axon_core::ui::{muted, primary};
use axon_services::debug as debug_service;
use std::error::Error;

pub async fn run_debug(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let user_context = if cfg.positional.is_empty() {
        String::new()
    } else {
        cfg.positional.join(" ")
    };
    let result = debug_service::debug_report(cfg, &user_context).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    let doctor_report = &result.payload["doctor_report"];
    // D1-09: LLM debug analysis is free text and may echo back config/env
    // values (or upstream error bodies) verbatim — redact secret-shaped
    // substrings before printing, same boundary the doctor renderer uses.
    let analysis_raw = result.payload["llm_debug"]["analysis"]
        .as_str()
        .unwrap_or("(no debug response)");
    let analysis = redact_secrets(analysis_raw);

    println!("{}", primary("Debug Snapshot"));
    println!(
        "  {} {}",
        muted("overall:"),
        if report_bool(doctor_report, &["all_ok"]) {
            "healthy"
        } else {
            "degraded"
        }
    );
    println!(
        "  {} {}",
        muted("tei:"),
        report_text(doctor_report, &["services", "tei", "model"], "")
    );
    println!(
        "  {} {}",
        muted("openai model:"),
        report_text(doctor_report, &["services", "openai", "model"], "")
    );
    println!();
    println!("{}", primary("LLM Debug"));
    println!("{analysis}");

    Ok(())
}

#[cfg(test)]
#[path = "debug_tests.rs"]
mod tests;
