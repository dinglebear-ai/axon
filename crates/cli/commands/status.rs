mod failure_summary;
pub(crate) mod metrics;
mod presentation;

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::services::system::{build_status_payload, load_status_jobs};
use std::error::Error;

pub async fn run_status(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info(&format!("command=status json={}", cfg.json_output));
    if cfg.json_output {
        // JSON path: route through the service layer for a stable payload shape.
        let result = crate::crates::services::system::full_status(cfg).await?;
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        // Human path: use the detailed per-job renderer for rich terminal output.
        run_status_impl(cfg).await?;
    }
    Ok(())
}

pub async fn status_snapshot(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let (jobs, totals) = load_status_jobs(cfg).await?;
    Ok(build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &jobs.refresh,
        &jobs.graph,
        &totals,
    ))
}

pub async fn status_text(cfg: &Config) -> Result<String, Box<dyn Error>> {
    let (_jobs, totals) = load_status_jobs(cfg).await?;
    let mut lines = Vec::new();
    lines.push("Axon Status".to_string());
    lines.push(format!("crawl jobs:   {} total", totals.crawl));
    lines.push(format!("extract jobs: {} total", totals.extract));
    lines.push(format!("embed jobs:   {} total", totals.embed));
    lines.push(format!("ingest jobs:  {} total", totals.ingest));
    lines.push(format!("refresh jobs: {} total", totals.refresh));
    lines.push(format!("graph jobs:   {} total", totals.graph));
    Ok(lines.join("\n"))
}

async fn run_status_impl(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let (jobs, totals) = load_status_jobs(cfg).await?;
    presentation::emit_status_human(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &jobs.refresh,
        &jobs.graph,
        &totals,
    );
    Ok(())
}
