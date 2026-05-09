mod failure_summary;
pub(crate) mod metrics;

use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{muted, primary, status_text as human_status_text, symbol_for_status};
use crate::services::context::ServiceContext;
use crate::services::system::{build_status_payload, load_status_jobs};
use crate::services::types::ServiceJob;
use std::collections::HashMap;
use std::error::Error;

pub async fn run_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!("command=status json={}", cfg.json_output));
    if cfg.json_output {
        // JSON path: route through the service layer for a stable payload shape.
        let result = crate::services::system::full_status(service_context).await?;
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
    let crawl_url_map: HashMap<uuid::Uuid, &str> = jobs
        .crawl
        .iter()
        .filter_map(|job| job.url.as_deref().map(|url| (job.id, url)))
        .collect();
    let embed_jobs_by_id: HashMap<String, &ServiceJob> = jobs
        .embed
        .iter()
        .map(|job| (job.id.to_string(), job))
        .collect();
    let embed_doc_totals = embed_doc_totals_from_crawls(&jobs.crawl);
    print_status_section(
        "Crawl",
        &jobs.crawl,
        |job| job.url.clone().unwrap_or_else(|| job.id.to_string()),
        |job| crawl_progress_summary(job, &embed_jobs_by_id, &embed_doc_totals),
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
        |job| {
            job.target
                .as_deref()
                .map(|target| metrics::display_embed_input(target, &crawl_url_map).into_owned())
                .unwrap_or_else(|| job.id.to_string())
        },
        |job| embed_progress_summary(job, embed_doc_totals.get(&job.id.to_string()).copied()),
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

fn crawl_progress_summary(
    job: &ServiceJob,
    embed_jobs_by_id: &HashMap<String, &ServiceJob>,
    embed_doc_totals: &HashMap<String, u64>,
) -> Option<String> {
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
            let mut summary = format!("{docs} docs · {time}");
            if let Some(embed_id) = metrics.get("embed_job_id").and_then(|v| v.as_str()) {
                if let Some(embed_job) = embed_jobs_by_id.get(embed_id) {
                    summary.push_str(&format!(" · embed {}", embed_job.status));
                    if let Some(embed_progress) =
                        embed_progress_summary(embed_job, embed_doc_totals.get(embed_id).copied())
                    {
                        summary.push_str(&format!(" ({embed_progress})"));
                    }
                } else {
                    summary.push_str(&format!(" · embed queued {embed_id}"));
                }
            }
            Some(summary)
        }
        _ => None,
    }
}

fn embed_progress_summary(job: &ServiceJob, fallback_docs_total: Option<u64>) -> Option<String> {
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
    let docs_total = metrics
        .get("docs_total")
        .and_then(|v| v.as_u64())
        .or(fallback_docs_total);
    if docs == 0 && chunks == 0 {
        return None;
    }
    if let Some(total) = docs_total.filter(|total| *total > 0) {
        let percent = ((docs as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
        let percent_text = if percent < 99.95 {
            format!("{percent:.1}%")
        } else {
            "100%".to_string()
        };
        return Some(format!(
            "{docs}/{total} docs · {percent_text} · {chunks} chunks"
        ));
    }
    if docs > 0 {
        Some(format!("{docs} docs · {chunks} chunks"))
    } else {
        Some(format!("{chunks} chunks"))
    }
}

fn embed_doc_totals_from_crawls(crawl_jobs: &[ServiceJob]) -> HashMap<String, u64> {
    crawl_jobs
        .iter()
        .filter_map(|job| {
            let metrics = job.result_json.as_ref()?;
            let embed_id = metrics.get("embed_job_id")?.as_str()?;
            let docs = metrics.get("md_created")?.as_u64()?;
            Some((embed_id.to_string(), docs))
        })
        .collect()
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
        if let Some(err) = job
            .error_text
            .as_deref()
            .and_then(|err| job_error_hint(&job.status, err))
        {
            println!("    {}", muted(&err));
        }
    }
    println!();
}

fn job_error_hint(status: &str, error_text: &str) -> Option<String> {
    if error_text.trim_start() == "reclaimed after unexpected shutdown" {
        return match status {
            "pending" => Some(
                "recovered after worker shutdown; waiting for a worker to claim it".to_string(),
            ),
            "running" => Some("recovered after worker shutdown; processing resumed".to_string()),
            "completed" => None,
            _ => Some(error_text.to_string()),
        };
    }
    Some(error_text.to_string())
}
