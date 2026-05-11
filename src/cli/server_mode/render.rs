use crate::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status,
};
use crate::cli::commands::crawl::subcommands::{
    render_list_subcommand as render_crawl_list, render_status_subcommand as render_crawl_status,
};
use crate::core::config::{CommandKind, Config};
use crate::core::ui::{accent, muted, primary};
use crate::services::types::{
    CrawlStartJob, CrawlStartResult, EmbedStartResult, ExtractStartResult, IngestStartResult,
    JobListResult, ScrapeResult, ScreenshotResult, ServiceJob, StartDisposition,
};
use std::error::Error;
use uuid::Uuid;

pub(super) fn render_server_result(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    match cfg.command {
        CommandKind::Status => {
            print!("{}", server_status_text(result)?);
            Ok(())
        }
        CommandKind::Scrape => render_scrape(cfg, result),
        CommandKind::Screenshot => render_screenshot(cfg, result),
        CommandKind::Crawl => render_crawl(cfg, label, result),
        CommandKind::Extract => render_extract(cfg, label, result),
        CommandKind::Embed => render_embed(cfg, label, result),
        CommandKind::Ingest => render_ingest(cfg, label, result, false),
        CommandKind::Sessions => render_ingest(cfg, label, result, true),
        _ => {
            println!("{}", serde_json::to_string_pretty(result)?);
            Ok(())
        }
    }
}

pub(super) fn server_status_text(result: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    crate::cli::commands::status::render_status_payload(result)
}

fn render_scrape(cfg: &Config, result: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let scrape: ScrapeResult = serde_json::from_value(result.clone())?;
    crate::cli::commands::scrape::print_scrape_preamble(cfg, &scrape.url);
    crate::cli::commands::scrape::emit_scrape_result(cfg, &scrape)
}

fn render_screenshot(cfg: &Config, result: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let payload = result
        .get("data")
        .cloned()
        .unwrap_or_else(|| result.clone());
    let shot: ScreenshotResult = serde_json::from_value(payload)?;
    crate::cli::commands::screenshot::print_screenshot_preamble(cfg, &shot.url);
    crate::cli::commands::screenshot::emit_screenshot_result(cfg, &shot.url, &shot)
}

fn render_crawl(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    if label != "job status" {
        match cfg.positional.first().map(String::as_str) {
            Some("status") => return render_crawl_status_view(cfg, result),
            Some("errors") => return render_generic_errors(cfg, result, "crawl"),
            Some("cancel") => return render_generic_cancel(cfg, result, "crawl"),
            Some("list") => return render_crawl_list_view(cfg, result),
            Some("cleanup") => return render_generic_cleanup(cfg, result, "crawl"),
            Some("clear") => return render_generic_clear(cfg, result, "crawl"),
            Some("recover") => return render_generic_recover(cfg, result, "crawl"),
            _ if cfg.wait => return Ok(()),
            _ => {}
        }
        let start = crawl_start_result(result)?;
        let display = match start.jobs.as_slice() {
            [single] => single.url.clone(),
            [first, rest @ ..] => format!("{} (+{} more)", first.url, rest.len()),
            [] => "crawl".to_string(),
        };
        crate::cli::commands::crawl::print_async_crawl_result(
            cfg,
            &display,
            &start.jobs,
            StartDisposition::Enqueued,
            true,
        );
        return Ok(());
    }
    render_crawl_status_view(cfg, result)
}

fn render_extract(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    if label != "job status" {
        match cfg.positional.first().map(String::as_str) {
            Some("status") => return render_generic_status(cfg, result, "Extract"),
            Some("errors") => return render_generic_errors(cfg, result, "extract"),
            Some("cancel") => return render_generic_cancel(cfg, result, "extract"),
            Some("list") => return render_generic_list(cfg, result, "Extract"),
            Some("cleanup") => return render_generic_cleanup(cfg, result, "extract"),
            Some("clear") => return render_generic_clear(cfg, result, "extract"),
            Some("recover") => return render_generic_recover(cfg, result, "extract"),
            _ if cfg.wait => return Ok(()),
            _ => {}
        }
        let start: ExtractStartResult = serde_json::from_value(result.clone())?;
        crate::cli::commands::extract::render_extract_enqueue_result(
            cfg,
            &start.job_id,
            StartDisposition::Enqueued,
            true,
        );
        return Ok(());
    }
    render_generic_status(cfg, result, "Extract")
}

fn render_embed(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    if label != "job status" {
        match cfg.positional.first().map(String::as_str) {
            Some("status") => return render_generic_status(cfg, result, "Embed"),
            Some("errors") => return render_generic_errors(cfg, result, "embed"),
            Some("cancel") => return render_generic_cancel(cfg, result, "embed"),
            Some("list") => return render_embed_list_view(cfg, result),
            Some("cleanup") => return render_generic_cleanup(cfg, result, "embed"),
            Some("clear") => return render_generic_clear(cfg, result, "embed"),
            Some("recover") => return render_generic_recover(cfg, result, "embed"),
            _ if cfg.wait => return Ok(()),
            _ => {}
        }
        let start: EmbedStartResult = serde_json::from_value(result.clone())?;
        let input = cfg.positional.first().cloned().unwrap_or_else(|| {
            cfg.output_dir
                .join("markdown")
                .to_string_lossy()
                .to_string()
        });
        crate::cli::commands::embed::render_embed_enqueue_result(
            cfg,
            &input,
            &start.job_id,
            StartDisposition::Enqueued,
            true,
        );
        return Ok(());
    }
    render_generic_status(cfg, result, "Embed")
}

fn render_ingest(
    cfg: &Config,
    label: &'static str,
    result: &serde_json::Value,
    sessions: bool,
) -> Result<(), Box<dyn Error>> {
    let command_name = if sessions { "sessions" } else { "ingest" };
    if label != "job status" {
        match cfg.positional.first().map(String::as_str) {
            Some("status") => return render_ingest_status_view(cfg, result),
            Some("errors") => return render_generic_errors(cfg, result, "ingest"),
            Some("cancel") => return render_generic_cancel(cfg, result, "ingest"),
            Some("list") => return render_ingest_list_view(cfg, result, command_name),
            Some("cleanup") => return render_generic_cleanup(cfg, result, "ingest"),
            Some("clear") => return render_generic_clear(cfg, result, "ingest"),
            Some("recover") => return render_generic_recover(cfg, result, "ingest"),
            _ if cfg.wait => return Ok(()),
            _ => {}
        }
        let start: IngestStartResult = serde_json::from_value(result.clone())?;
        if sessions {
            println!("  {} {}", primary("Ingest Job"), accent(&start.job_id));
            println!("  {}", muted("Status: pending"));
            println!("  {} {}", muted("Collection:"), accent(&cfg.collection));
            println!("Job ID: {}", start.job_id);
        } else {
            crate::cli::commands::ingest::render_ingest_enqueue_result(
                cfg,
                &start.job_id,
                StartDisposition::Enqueued,
                true,
            )?;
        }
        return Ok(());
    }
    render_ingest_status_view(cfg, result)
}

fn render_generic_status(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let job = maybe_job(result)?;
    let id = job_id_from_result_or_cfg(result, cfg)?;
    handle_job_status(cfg, job, id, command_name)
}

fn render_generic_errors(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let job = maybe_job(result)?;
    let id = job_id_from_result_or_cfg(result, cfg)?;
    handle_job_errors(cfg, job, id, command_name)
}

fn render_generic_cancel(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let id = job_id_from_result_or_cfg(result, cfg)?;
    let canceled = bool_field(result, "canceled")?;
    handle_job_cancel(cfg, id, canceled, command_name)
}

fn render_generic_list(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let jobs = jobs_from_result(result)?;
    let total = jobs.len() as i64;
    let list = JobListResult::new(jobs, total, 50, 0);
    handle_job_list(cfg, &list, command_name)
}

fn render_generic_cleanup(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    handle_job_cleanup(
        cfg,
        numeric_field(result, &["deleted", "removed"])?,
        command_name,
    )
}

fn render_generic_clear(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    handle_job_clear(
        cfg,
        numeric_field(result, &["deleted", "removed"])?,
        command_name,
    )
}

fn render_generic_recover(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    handle_job_recover(
        cfg,
        numeric_field(result, &["recovered", "reclaimed"])?,
        command_name,
    )
}

fn render_crawl_status_view(
    cfg: &Config,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let job = maybe_job(result)?;
    let id = job_id_from_result_or_cfg(result, cfg)?;
    render_crawl_status(cfg, job, id)
}

fn render_crawl_list_view(cfg: &Config, result: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let jobs = jobs_from_result(result)?;
    let total = jobs.len() as i64;
    render_crawl_list(cfg, jobs, total)
}

fn render_embed_list_view(cfg: &Config, result: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let jobs = jobs_from_result(result)?;
    let total = jobs.len() as i64;
    crate::cli::commands::embed::render_embed_list(cfg, jobs, total)
}

fn render_ingest_status_view(
    cfg: &Config,
    result: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let job = maybe_job(result)?;
    let id = job_id_from_result_or_cfg(result, cfg)?;
    crate::cli::commands::ingest_common::render_ingest_status(cfg, job, id)
}

fn render_ingest_list_view(
    cfg: &Config,
    result: &serde_json::Value,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let jobs = jobs_from_result(result)?;
    let total = jobs.len() as i64;
    crate::cli::commands::ingest_common::render_ingest_list(cfg, jobs, total, command_name)
}

fn maybe_job(result: &serde_json::Value) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    match result.get("job") {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
        None => Ok(None),
    }
}

fn jobs_from_result(result: &serde_json::Value) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    match result.get("jobs") {
        Some(value) => Ok(serde_json::from_value(value.clone())?),
        None => Ok(Vec::new()),
    }
}

fn crawl_start_result(result: &serde_json::Value) -> Result<CrawlStartResult, Box<dyn Error>> {
    let jobs = result
        .get("jobs")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(vec![]));
    let jobs: Vec<CrawlStartJob> = serde_json::from_value(jobs)?;
    let job_ids = result
        .get("job_ids")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(vec![]));
    let job_ids: Vec<String> = serde_json::from_value(job_ids)?;
    Ok(CrawlStartResult {
        job_ids,
        output_dir: result
            .get("output_dir")
            .and_then(|value| value.as_str().map(ToString::to_string)),
        predicted_paths: result
            .get("predicted_paths")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default(),
        predicted_artifact_handles: Vec::new(),
        jobs,
    })
}

fn job_id_from_result_or_cfg(
    result: &serde_json::Value,
    cfg: &Config,
) -> Result<Uuid, Box<dyn Error>> {
    if let Some(id) = result
        .get("job_id")
        .and_then(|value| value.as_str())
        .or_else(|| {
            result
                .get("job")
                .and_then(|job| job.get("id"))
                .and_then(|value| value.as_str())
        })
    {
        return Ok(Uuid::parse_str(id)?);
    }
    let id = cfg
        .positional
        .get(1)
        .ok_or("missing <job-id> in CLI arguments")?;
    Ok(Uuid::parse_str(id)?)
}

fn numeric_field(result: &serde_json::Value, names: &[&str]) -> Result<u64, Box<dyn Error>> {
    for name in names {
        if let Some(value) = result.get(*name).and_then(|value| value.as_u64()) {
            return Ok(value);
        }
    }
    Err(format!("missing numeric field: {}", names.join(" or ")).into())
}

fn bool_field(result: &serde_json::Value, name: &str) -> Result<bool, Box<dyn Error>> {
    result
        .get(name)
        .and_then(|value| value.as_bool())
        .ok_or_else(|| format!("missing boolean field: {name}").into())
}
