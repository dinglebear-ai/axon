mod failure_summary;
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
    let (jobs, totals) = load_status_jobs(service_context).await?;
    Ok(build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
    ))
}

pub async fn status_text(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<String, Box<dyn Error>> {
    let (_jobs, totals) = load_status_jobs(service_context).await?;
    let mut lines = Vec::new();
    lines.push("Axon Status".to_string());
    lines.push(format!("crawl jobs:   {} total", totals.crawl));
    lines.push(format!("extract jobs: {} total", totals.extract));
    lines.push(format!("embed jobs:   {} total", totals.embed));
    lines.push(format!("ingest jobs:  {} total", totals.ingest));
    Ok(lines.join("\n"))
}

async fn run_status_impl(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let (jobs, _totals) = load_status_jobs(service_context).await?;
    print_status_section(
        "Crawl",
        &jobs.crawl,
        |job| job.url.clone().unwrap_or_else(|| job.id.to_string()),
        crawl_progress_summary,
    );
    print_status_section(
        "Extract",
        &jobs.extract,
        |job| {
            job.urls_json
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| job.id.to_string())
        },
        extract_progress_summary,
    );
    print_status_section(
        "Embed",
        &jobs.embed,
        |job| job.target.clone().unwrap_or_else(|| job.id.to_string()),
        embed_progress_summary,
    );
    print_status_section(
        "Ingest",
        &jobs.ingest,
        |job| match (&job.source_type, &job.target) {
            (Some(source_type), Some(target)) => format!("{source_type}: {target}"),
            (_, Some(target)) => target.clone(),
            _ => job.id.to_string(),
        },
        ingest_progress_summary,
    );
    Ok(())
}

fn crawl_progress_summary(job: &ServiceJob) -> Option<String> {
    let metrics = job.result_json.as_ref()?;
    match job.status.as_str() {
        "running" => {
            let crawled = metrics
                .get("pages_crawled")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let docs = metrics
                .get("md_created")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if crawled == 0 && docs == 0 {
                return None;
            }
            if docs > 0 {
                Some(format!("{crawled} crawled · {docs} docs"))
            } else {
                Some(format!("{crawled} crawled"))
            }
        }
        "completed" => {
            let docs = metrics
                .get("md_created")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let elapsed_ms = metrics
                .get("elapsed_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let time = if elapsed_ms >= 1000 {
                format!("{:.1}s", elapsed_ms as f64 / 1000.0)
            } else {
                format!("{elapsed_ms}ms")
            };
            Some(format!("{docs} docs · {time}"))
        }
        _ => None,
    }
}

fn embed_progress_summary(job: &ServiceJob) -> Option<String> {
    let metrics = job.result_json.as_ref()?;
    if !matches!(job.status.as_str(), "running" | "completed") {
        return None;
    }
    let docs = metrics
        .get("docs_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let chunks = metrics
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if docs == 0 && chunks == 0 {
        return None;
    }
    if docs > 0 {
        Some(format!("{docs} docs · {chunks} chunks"))
    } else {
        Some(format!("{chunks} chunks"))
    }
}

fn extract_progress_summary(job: &ServiceJob) -> Option<String> {
    let metrics = job.result_json.as_ref()?;
    if !matches!(job.status.as_str(), "running" | "completed") {
        return None;
    }
    let items = metrics
        .get("total_items")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if items == 0 {
        return None;
    }
    Some(format!("{items} items"))
}

fn ingest_progress_summary(job: &ServiceJob) -> Option<String> {
    let metrics = job.result_json.as_ref()?;
    if !matches!(job.status.as_str(), "running" | "completed") {
        return None;
    }
    let chunks = metrics
        .get("chunks")
        .or_else(|| metrics.get("chunks_embedded"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if chunks == 0 {
        return None;
    }
    Some(format!("{chunks} chunks"))
}

fn print_status_section(
    title: &str,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
    progress_for: impl Fn(&ServiceJob) -> Option<String>,
) {
    println!("{}", primary(title));
    if jobs.is_empty() {
        println!("  {}", muted("None."));
        println!();
        return;
    }

    for job in jobs.iter().take(10) {
        let label = label_for(job);
        if let Some(p) = progress_for(job) {
            println!(
                "  {} {} {} {}  {}",
                symbol_for_status(&job.status),
                human_status_text(&job.status),
                label,
                muted(&job.id.to_string()),
                muted(&p),
            );
        } else {
            println!(
                "  {} {} {} {}",
                symbol_for_status(&job.status),
                human_status_text(&job.status),
                label,
                muted(&job.id.to_string()),
            );
        }
        if let Some(err) = job.error_text.as_deref() {
            println!("    {}", muted(err));
        }
    }
    println!();
}
