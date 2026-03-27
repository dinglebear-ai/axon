pub(crate) mod metrics;

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::system::{build_status_payload, load_status_jobs};
use std::error::Error;

pub async fn run_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!("command=status json={}", cfg.json_output));
    if cfg.json_output {
        // JSON path: route through the service layer for a stable payload shape.
        let result = crate::crates::services::system::full_status(service_context).await?;
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        // Human path: use the detailed per-job renderer for rich terminal output.
        run_status_impl(cfg, service_context).await?;
    }
    Ok(())
}

pub async fn status_snapshot(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let jobs = load_status_jobs(service_context).await?;
    Ok(build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &jobs.refresh,
        &jobs.graph,
    ))
}

pub async fn status_text(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<String, Box<dyn Error>> {
    let jobs = load_status_jobs(service_context).await?;
    let mut lines = Vec::new();
    lines.push("Axon Status".to_string());
    lines.push(format!("crawl jobs:   {}", jobs.crawl.len()));
    lines.push(format!("extract jobs: {}", jobs.extract.len()));
    lines.push(format!("embed jobs:   {}", jobs.embed.len()));
    lines.push(format!("ingest jobs:  {}", jobs.ingest.len()));
    lines.push(format!("refresh jobs: {}", jobs.refresh.len()));
    lines.push(format!("graph jobs:   {}", jobs.graph.len()));
    Ok(lines.join("\n"))
}

async fn run_status_impl(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    println!("{}", status_text(cfg, service_context).await?);
    Ok(())
}
