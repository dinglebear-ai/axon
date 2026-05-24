use crate::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status,
};
use crate::cli::commands::crawl::subcommands::{
    render_list_subcommand as render_crawl_list, render_status_subcommand as render_crawl_status,
};
use crate::core::config::Config;
use crate::core::ui::{accent, muted, primary};
use crate::services::types::{
    CrawlStartJob, CrawlStartResult, EmbedStartResult, ExtractStartResult, ExtractSyncResult,
    IngestStartResult, JobListResult, ServiceJob, StartDisposition,
};
use std::error::Error;
use uuid::Uuid;

pub(super) fn render_crawl(
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

pub(super) fn render_extract(
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
    if let Some(extract_result) = completed_extract_sync_result(result)? {
        return crate::cli::commands::extract::emit_extract_output(cfg, &extract_result);
    }
    render_generic_status(cfg, result, "Extract")
}

pub(super) fn render_embed(
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

pub(super) fn render_ingest(
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
    let (total, limit, offset) = list_pagination_from_result(result, jobs.len() as i64);
    let list = JobListResult::new(jobs, total, limit, offset);
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
    let (total, _, _) = list_pagination_from_result(result, jobs.len() as i64);
    render_crawl_list(cfg, jobs, total)
}

fn render_embed_list_view(cfg: &Config, result: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let jobs = jobs_from_result(result)?;
    let (total, _, _) = list_pagination_from_result(result, jobs.len() as i64);
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
    let (total, _, _) = list_pagination_from_result(result, jobs.len() as i64);
    crate::cli::commands::ingest_common::render_ingest_list(cfg, jobs, total, command_name)
}

fn maybe_job(result: &serde_json::Value) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    match result.get("job") {
        Some(value) if value.is_null() => Ok(None),
        Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
        None if looks_like_service_job(result) => Ok(Some(serde_json::from_value(result.clone())?)),
        None => Ok(None),
    }
}

fn looks_like_service_job(result: &serde_json::Value) -> bool {
    result.get("id").is_some() && result.get("status").is_some()
}

pub(crate) fn extract_status_json_result(result: &serde_json::Value) -> serde_json::Value {
    let mut output = result.clone();
    if let Some(extract_result) = result
        .get("job")
        .and_then(|job| job.get("result_json"))
        .filter(|value| !value.is_null())
        && let Some(object) = output.as_object_mut()
    {
        object.insert("extract_result".to_string(), extract_result.clone());
    }
    output
}

fn completed_extract_sync_result(
    result: &serde_json::Value,
) -> Result<Option<ExtractSyncResult>, Box<dyn Error>> {
    let Some(job) = maybe_job(result)? else {
        return Ok(None);
    };
    if job.status != "completed" {
        return Ok(None);
    }
    let Some(summary) = job.result_json else {
        return Ok(None);
    };
    let summary_path = summary
        .get("summary_path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let items_path = summary
        .get("items_path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let total_items = summary
        .get("total_items")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let duration_ms = summary
        .get("duration_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u128;
    Ok(Some(ExtractSyncResult {
        summary,
        summary_path,
        items_path,
        total_items,
        duration_ms,
    }))
}

fn jobs_from_result(result: &serde_json::Value) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    match result.get("jobs") {
        Some(value) => Ok(serde_json::from_value(value.clone())?),
        None => Ok(Vec::new()),
    }
}

fn list_pagination_from_result(result: &serde_json::Value, fallback_len: i64) -> (i64, i64, i64) {
    let total = i64_field(result, "total").unwrap_or(fallback_len);
    let limit = i64_field(result, "limit").unwrap_or(fallback_len.max(1));
    let offset = i64_field(result, "offset").unwrap_or(0);
    (total, limit, offset)
}

fn i64_field(result: &serde_json::Value, name: &str) -> Option<i64> {
    result.get(name).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
    })
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
