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
use std::fmt::Write as _;

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
    print!("{}", render_status_jobs(&jobs));
    Ok(())
}

pub(crate) fn render_status_payload(payload: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    #[derive(serde::Deserialize)]
    struct StatusPayload {
        local_crawl_jobs: Vec<ServiceJob>,
        local_extract_jobs: Vec<ServiceJob>,
        local_embed_jobs: Vec<ServiceJob>,
        local_ingest_jobs: Vec<ServiceJob>,
    }

    let payload: StatusPayload = serde_json::from_value(payload.clone())?;
    Ok(render_status_jobs_from_slices(
        &payload.local_crawl_jobs,
        &payload.local_extract_jobs,
        &payload.local_embed_jobs,
        &payload.local_ingest_jobs,
    ))
}

fn render_status_jobs(jobs: &crate::services::system::StatusJobs) -> String {
    render_status_jobs_from_slices(&jobs.crawl, &jobs.extract, &jobs.embed, &jobs.ingest)
}

fn render_status_jobs_from_slices(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
) -> String {
    let crawl_url_map: HashMap<uuid::Uuid, &str> = crawl_jobs
        .iter()
        .filter_map(|job| {
            let url = job.url.as_deref()?;
            Some((job.id, url))
        })
        .collect();
    let embed_jobs_by_id: HashMap<String, &ServiceJob> = embed_jobs
        .iter()
        .map(|job| (job.id.to_string(), job))
        .collect();
    let embed_doc_totals = embed_doc_totals_from_crawls(crawl_jobs);
    let mut out = String::new();
    write_status_section(
        &mut out,
        "Crawl",
        crawl_jobs,
        |job| job.url.clone().unwrap_or_else(|| job.id.to_string()),
        |job| crawl_progress_summary(job, &embed_jobs_by_id, &embed_doc_totals),
    );
    write_status_section(
        &mut out,
        "Extract",
        extract_jobs,
        |job| {
            job.urls_json
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| job.id.to_string())
        },
        extract_progress_summary,
    );
    write_status_section(
        &mut out,
        "Embed",
        embed_jobs,
        |job| {
            job.target
                .as_deref()
                .map(|target| metrics::display_embed_input(target, &crawl_url_map).into_owned())
                .unwrap_or_else(|| job.id.to_string())
        },
        |job| embed_progress_summary(job, embed_doc_totals.get(&job.id.to_string()).copied()),
    );
    write_status_section(
        &mut out,
        "Ingest",
        ingest_jobs,
        |job| match (&job.source_type, &job.target) {
            (Some(source_type), Some(target)) => format!("{source_type}: {target}"),
            (_, Some(target)) => target.clone(),
            _ => job.id.to_string(),
        },
        ingest_progress_summary,
    );
    out
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

fn write_status_section(
    out: &mut String,
    title: &str,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
    progress_for: impl Fn(&ServiceJob) -> Option<String>,
) {
    let _ = writeln!(out, "{}", primary(title));
    if jobs.is_empty() {
        let _ = writeln!(out, "  {}", muted("None."));
        let _ = writeln!(out);
        return;
    }

    for job in jobs.iter().take(10) {
        let label = label_for(job);
        if let Some(p) = progress_for(job) {
            let _ = writeln!(
                out,
                "  {} {} {} {}  {}",
                symbol_for_status(&job.status),
                human_status_text(&job.status),
                label,
                muted(&job.id.to_string()),
                muted(&p),
            );
        } else {
            let _ = writeln!(
                out,
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
            let _ = writeln!(out, "    {}", muted(&err));
        }
    }
    let _ = writeln!(out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::system::StatusJobs;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn job(status: &str) -> ServiceJob {
        ServiceJob {
            id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            status: status.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            finished_at: None,
            error_text: None,
            url: Some("https://example.com/docs".to_string()),
            source_type: None,
            target: Some("https://example.com/docs".to_string()),
            urls_json: None,
            result_json: Some(json!({
                "pages_crawled": 3,
                "md_created": 2,
                "elapsed_ms": 1200,
                "docs_embedded": 2,
                "docs_total": 2,
                "chunks_embedded": 8
            })),
            config_json: None,
        }
    }

    #[test]
    fn render_status_payload_matches_local_renderer() {
        let jobs = StatusJobs {
            crawl: vec![job("completed")],
            extract: Vec::new(),
            embed: vec![job("completed")],
            ingest: Vec::new(),
        };
        let payload = build_status_payload(
            &jobs.crawl,
            &jobs.extract,
            &jobs.embed,
            &jobs.ingest,
            &crate::services::types::StatusTotals::default(),
        );

        let from_jobs = render_status_jobs(&jobs);
        let from_payload = render_status_payload(&payload).expect("payload should render");

        assert_eq!(from_payload, from_jobs);
        assert!(from_payload.contains("Crawl"));
        assert!(from_payload.contains("Embed"));
        assert!(from_payload.contains("2 docs"));
    }
}
