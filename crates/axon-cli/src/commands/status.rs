mod watch;

use crate::commands::job_progress::{extract_progress_summary, source_progress_summary};
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::redact::redact_secrets;
use axon_core::ui::{muted, primary, status_text as human_status_text, symbol_for_status};
use axon_jobs::store::{RECLAIMED_ERROR_TEXT, sqlite_diagnostics};
use axon_services::context::ServiceContext;
use axon_services::system::{
    build_status_payload_with_errors_and_sqlite, load_status_jobs, sqlite_status_error,
};
use axon_services::types::ServiceJob;
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
        let result = axon_services::system::full_status(service_context).await?;
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        // Human path: use the detailed per-job renderer for rich terminal output.
        run_status_impl(cfg, service_context).await?;
    }
    Ok(())
}

pub async fn status_snapshot(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let (jobs, totals, mut errors) = load_status_jobs(service_context).await?;
    let sqlite = sqlite_diagnostics(&cfg.sqlite_path).await;
    if let Some(error) = sqlite_status_error(&sqlite) {
        errors.push(error);
    }
    Ok(build_status_payload_with_errors_and_sqlite(
        &jobs.source,
        &jobs.extract,
        &jobs.watch,
        &jobs.prune,
        &totals,
        &errors,
        &sqlite,
    ))
}

pub async fn status_text(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<String, Box<dyn Error>> {
    let (_jobs, totals, mut errors) = load_status_jobs(service_context).await?;
    let sqlite = sqlite_diagnostics(&cfg.sqlite_path).await;
    if let Some(error) = sqlite_status_error(&sqlite) {
        errors.push(error);
    }
    let mut lines = Vec::new();
    lines.push("Axon Status".to_string());
    lines.push(format!("source jobs:  {} total", totals.source));
    lines.push(format!("extract jobs: {} total", totals.extract));
    lines.push(format!("watch jobs:   {} total", totals.watch));
    lines.push(format!("prune jobs:   {} total", totals.prune));
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
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let (jobs, totals, mut errors) = load_status_jobs(service_context).await?;
    let sqlite = sqlite_diagnostics(&cfg.sqlite_path).await;
    if let Some(error) = sqlite_status_error(&sqlite) {
        errors.push(error);
    }
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
    print!("{}", render_status_jobs(&jobs, totals.source));
    Ok(())
}

fn render_status_jobs(jobs: &axon_services::system::StatusJobs, source_total: i64) -> String {
    let source_note = source_truncation_note(jobs.source.len(), source_total.max(0));
    render_status_jobs_from_slices(
        &jobs.source,
        &jobs.extract,
        &jobs.watch,
        &jobs.prune,
        source_note.as_deref(),
    )
}

/// Returns "showing N of M …" when the renderer will hide rows. N reflects
/// what `write_status_section` will actually display (capped at
/// `SECTION_DISPLAY_LIMIT`), so the note never advertises a count the
/// renderer won't show.
fn source_truncation_note(slice_len: usize, total: i64) -> Option<String> {
    let displayed = slice_len.min(SECTION_DISPLAY_LIMIT);
    let displayed_i64 = i64::try_from(displayed).unwrap_or(i64::MAX);
    (total > displayed_i64)
        .then(|| format!("showing {displayed} of {total} total · running jobs listed first"))
}

fn render_status_jobs_from_slices(
    source_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    watch_jobs: &[ServiceJob],
    prune_jobs: &[ServiceJob],
    source_note: Option<&str>,
) -> String {
    let mut out = String::new();
    write_status_section(
        &mut out,
        "Source",
        source_note,
        source_jobs,
        format_subject,
        source_progress_summary,
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
        "Watch",
        None,
        watch_jobs,
        format_subject,
        source_progress_summary,
    );
    write_status_section(
        &mut out,
        "Prune",
        None,
        prune_jobs,
        format_subject,
        source_progress_summary,
    );
    out
}

fn format_subject(job: &ServiceJob) -> String {
    match (
        job.url.as_deref(),
        job.source_type.as_deref(),
        job.target.as_deref(),
        job.urls_json.as_ref(),
    ) {
        (Some(url), _, _, _) => url.to_string(),
        (None, Some(source_type), Some(target), _) => format!("{source_type}: {target}"),
        (None, _, Some(target), _) => target.to_string(),
        (None, _, _, Some(urls)) => urls.to_string(),
        _ => job.id.to_string(),
    }
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
        // D1-09: job labels/targets and error text are DB-persisted strings
        // that may embed URL credentials or upstream error bodies with
        // tokens — redact before display, same boundary the doctor renderer
        // uses (see doctor/render.rs::report_text).
        let label = truncate_status_text_to(&redact_secrets(&label_for(job)), label_limit);
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
            let err = redact_secrets(&err);
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
        crate::commands::common::truncate_chars(text, max_chars - 1)
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
