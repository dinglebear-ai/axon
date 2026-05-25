mod failure_summary;
pub(crate) mod metrics;
mod watch;

use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{muted, primary, status_text as human_status_text, symbol_for_status};
use crate::jobs::store::RECLAIMED_ERROR_TEXT;
use crate::services::context::ServiceContext;
use crate::services::system::{build_status_payload_with_errors, load_status_jobs};
use crate::services::types::ServiceJob;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;

/// Maximum number of rows rendered per section in the human status output.
/// The truncation note ("showing N of M") is sized against this cap.
const SECTION_DISPLAY_LIMIT: usize = 10;
const STATUS_TEXT_DISPLAY_LIMIT: usize = 120;
const STATUS_CONTINUATION_INDENT: usize = 4;

pub async fn run_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!("command=status json={}", cfg.json_output));
    // Watch mode is entirely progress output (MultiProgress + ProgressBar
    // spinners), so suppress it under --quiet — the flag's contract is to
    // hide spinners/progress for scripted use.
    if cfg.watch_mode && !cfg.json_output && !cfg.quiet {
        return watch::run_status_watch(cfg, service_context).await;
    }
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
    let (jobs, totals, errors) = load_status_jobs(service_context).await?;
    Ok(build_status_payload_with_errors(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
        &errors,
    ))
}

pub async fn status_text(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<String, Box<dyn Error>> {
    let (_jobs, totals, errors) = load_status_jobs(service_context).await?;
    let mut lines = Vec::new();
    lines.push("Axon Status".to_string());
    lines.push(format!("crawl jobs:   {} total", totals.crawl));
    lines.push(format!("extract jobs: {} total", totals.extract));
    lines.push(format!("embed jobs:   {} total", totals.embed));
    lines.push(format!("ingest jobs:  {} total", totals.ingest));
    if !errors.is_empty() {
        lines.push(format!(
            "degraded: {} status count error{}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        ));
    }
    Ok(lines.join("\n"))
}

async fn run_status_impl(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let (jobs, totals, errors) = load_status_jobs(service_context).await?;
    if !errors.is_empty() {
        println!(
            "{}",
            muted(&format!(
                "Status degraded: {} count query error{}",
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            ))
        );
        for error in &errors {
            println!("  {}", muted(error));
        }
        println!();
    }
    print!("{}", render_status_jobs(&jobs, totals.crawl));
    Ok(())
}

pub(crate) fn render_status_payload(value: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    #[derive(serde::Deserialize)]
    struct StatusPayload {
        local_crawl_jobs: Vec<ServiceJob>,
        local_extract_jobs: Vec<ServiceJob>,
        local_embed_jobs: Vec<ServiceJob>,
        local_ingest_jobs: Vec<ServiceJob>,
    }

    // Unwrap the StatusResult envelope ({payload, text, totals}) if the server
    // returned the full struct rather than just the inner payload JSON.
    let inner = value
        .get("payload")
        .filter(|p| p.get("local_crawl_jobs").is_some())
        .unwrap_or(value);

    // Defensive: clamp negative totals to 0 so a malformed payload can't
    // either suppress the truncation note or panic the formatter.
    let crawl_total = inner
        .get("totals")
        .or_else(|| value.get("totals"))
        .and_then(|t| t.get("crawl"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);

    let parsed: StatusPayload = serde_json::from_value(inner.clone())?;
    let crawl_note = crawl_truncation_note(parsed.local_crawl_jobs.len(), crawl_total);
    Ok(render_status_jobs_from_slices(
        &parsed.local_crawl_jobs,
        &parsed.local_extract_jobs,
        &parsed.local_embed_jobs,
        &parsed.local_ingest_jobs,
        crawl_note.as_deref(),
    ))
}

fn render_status_jobs(jobs: &crate::services::system::StatusJobs, crawl_total: i64) -> String {
    let crawl_note = crawl_truncation_note(jobs.crawl.len(), crawl_total.max(0));
    render_status_jobs_from_slices(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        crawl_note.as_deref(),
    )
}

/// Returns "showing N of M …" when the renderer will hide rows. N reflects
/// what `write_status_section` will actually display (capped at
/// `SECTION_DISPLAY_LIMIT`), so the note never advertises a count the
/// renderer won't show.
fn crawl_truncation_note(slice_len: usize, total: i64) -> Option<String> {
    let displayed = slice_len.min(SECTION_DISPLAY_LIMIT);
    let displayed_i64 = i64::try_from(displayed).unwrap_or(i64::MAX);
    (total > displayed_i64)
        .then(|| format!("showing {displayed} of {total} total · running jobs listed first"))
}

fn render_status_jobs_from_slices(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    crawl_note: Option<&str>,
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
        crawl_note,
        crawl_jobs,
        |job| job.url.clone().unwrap_or_else(|| job.id.to_string()),
        |job| crawl_progress_summary(job, &embed_jobs_by_id, &embed_doc_totals),
    );
    write_status_section(
        &mut out,
        "Extract",
        None,
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
        None,
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
        None,
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
            let errors = metrics
                .get("error_pages")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let reclaim = reclaimed_suffix(job);
            let error_suffix = if errors > 0 {
                format!(" · {errors} errors")
            } else {
                String::new()
            };
            if docs > 0 {
                Some(format!(
                    "{crawled} crawled · {docs} docs{error_suffix}{reclaim}"
                ))
            } else {
                Some(format!("{crawled} crawled{error_suffix}{reclaim}"))
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
        "pending" => reclaimed_suffix(job)
            .strip_prefix(" · ")
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn reclaimed_suffix(job: &ServiceJob) -> String {
    match job.error_text.as_deref().map(str::trim_start) {
        Some(RECLAIMED_ERROR_TEXT) => " · reclaimed retry".to_string(),
        _ => String::new(),
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
    section_note: Option<&str>,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
    progress_for: impl Fn(&ServiceJob) -> Option<String>,
) {
    let _ = writeln!(out, "{}", primary(title));
    if let Some(note) = section_note {
        let _ = writeln!(out, "  {}", muted(note));
    }
    if jobs.is_empty() {
        let _ = writeln!(out, "  {}", muted("None."));
        let _ = writeln!(out);
        return;
    }

    for job in jobs.iter().take(SECTION_DISPLAY_LIMIT) {
        let status = human_status_text(&job.status);
        let prefix = format!("  {} {} ", symbol_for_status(&job.status), status);
        let label_limit = STATUS_TEXT_DISPLAY_LIMIT.saturating_sub(prefix.chars().count());
        let label = truncate_status_text_to(&label_for(job), label_limit);
        let _ = writeln!(out, "{prefix}{label}");
        let _ = writeln!(out, "    {}", muted(&format!("id {}", job.id)));
        if let Some(p) = progress_for(job) {
            let _ = writeln!(out, "    {}", muted(&truncate_status_continuation(&p)));
        }
        if let Some(err) = job
            .error_text
            .as_deref()
            .and_then(|err| job_error_hint(&job.status, err))
        {
            let _ = writeln!(out, "    {}", muted(&truncate_status_continuation(&err)));
        }
    }
    let _ = writeln!(out);
}

fn truncate_status_continuation(text: &str) -> String {
    truncate_status_text_to(
        text,
        STATUS_TEXT_DISPLAY_LIMIT.saturating_sub(STATUS_CONTINUATION_INDENT),
    )
}

fn truncate_status_text_to(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    format!(
        "{}…",
        crate::cli::commands::common::truncate_chars(text, max_chars - 1)
    )
}

fn job_error_hint(status: &str, error_text: &str) -> Option<String> {
    if error_text.trim_start() == RECLAIMED_ERROR_TEXT {
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
#[path = "status_tests.rs"]
mod tests;
