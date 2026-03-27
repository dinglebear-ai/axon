pub(crate) mod metrics;

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{
    muted, primary, status_text as human_status_text, symbol_for_status,
};
use crate::crates::services::context::ServiceContext;
use crate::crates::services::system::{build_status_payload, load_status_jobs};
use crate::crates::services::types::ServiceJob;
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
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let jobs = load_status_jobs(service_context).await?;
    print_status_section("Crawl", &jobs.crawl, |job| {
        job.url.clone().unwrap_or_else(|| job.id.to_string())
    });
    print_status_section("Extract", &jobs.extract, |job| {
        job.urls_json
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| job.id.to_string())
    });
    print_status_section("Embed", &jobs.embed, |job| {
        job.target.clone().unwrap_or_else(|| job.id.to_string())
    });
    print_status_section("Ingest", &jobs.ingest, |job| {
        match (&job.source_type, &job.target) {
            (Some(source_type), Some(target)) => format!("{source_type}: {target}"),
            (_, Some(target)) => target.clone(),
            _ => job.id.to_string(),
        }
    });
    print_status_section("Refresh", &jobs.refresh, |job| {
        job.target
            .clone()
            .or_else(|| job.url.clone())
            .unwrap_or_else(|| job.id.to_string())
    });
    print_status_section("Graph", &jobs.graph, |job| {
        job.url.clone().unwrap_or_else(|| job.id.to_string())
    });
    Ok(())
}

fn print_status_section(
    title: &str,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
) {
    println!("{}", primary(title));
    if jobs.is_empty() {
        println!("  {}", muted("None."));
        println!();
        return;
    }

    for job in jobs.iter().take(10) {
        println!(
            "  {} {} {} {}",
            symbol_for_status(&job.status),
            human_status_text(&job.status),
            label_for(job),
            muted(&job.id.to_string()),
        );
        if let Some(err) = job.error_text.as_deref() {
            println!("    {}", muted(err));
        }
    }
    println!();
}
