use crate::crates::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status,
};
use crate::crates::cli::commands::status::metrics::{
    collection_from_config, display_embed_input, embed_metrics_suffix, format_error,
    job_runtime_text,
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::core::ui::{
    accent, confirm_destructive, error, muted, primary, status_label, subtle, symbol_for_status,
};
use crate::crates::services::embed as embed_service;
use std::error::Error;
use std::path::Path;
use uuid::Uuid;

pub async fn run_embed(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if maybe_handle_embed_subcommand(cfg).await? {
        return Ok(());
    }

    log_info(&format!(
        "command=embed collection={} wait={}",
        cfg.collection, cfg.wait
    ));
    let embed_start = std::time::Instant::now();
    let input = resolve_embed_input(cfg);
    if !cfg.wait {
        let result = enqueue_embed_job(cfg, &input).await;
        if result.is_ok() {
            log_info(&format!(
                "job_enqueued command=embed queue={}",
                cfg.embed_queue
            ));
        }
        return result;
    }

    embed_service::embed_now(cfg, &input).await?;
    log_done(&format!(
        "command=embed complete collection={} duration_ms={}",
        cfg.collection,
        embed_start.elapsed().as_millis()
    ));
    Ok(())
}

async fn maybe_handle_embed_subcommand(cfg: &Config) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };
    if cfg.positional.len() == 1 && Path::new(subcmd).exists() {
        // Allow embedding a local path literally named like a subcommand
        // (for example: "./status").
        return Ok(false);
    }

    match subcmd {
        "status" => handle_embed_status(cfg).await?,
        "cancel" => handle_embed_cancel(cfg).await?,
        "errors" => handle_embed_errors(cfg).await?,
        "list" => handle_embed_list(cfg).await?,
        "cleanup" => handle_embed_cleanup(cfg).await?,
        "clear" => handle_embed_clear(cfg).await?,
        "worker" => crate::crates::jobs::embed::run_embed_worker(cfg).await?,
        "recover" => handle_embed_recover(cfg).await?,
        _ => return Ok(false),
    }

    Ok(true)
}

fn parse_embed_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("embed {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

async fn handle_embed_status(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "status")?;
    let job = crate::crates::jobs::embed::get_embed_job(cfg, id).await?;
    handle_job_status(cfg, job, id, "Embed")
}

async fn handle_embed_cancel(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "cancel")?;
    let canceled = embed_service::embed_cancel(cfg, id).await?;
    handle_job_cancel(cfg, id, canceled, "embed")
}

async fn handle_embed_errors(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "errors")?;
    let job = crate::crates::jobs::embed::get_embed_job(cfg, id).await?;
    handle_job_errors(cfg, job, id, "embed")
}

async fn handle_embed_list(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let jobs = crate::crates::jobs::embed::list_embed_jobs(cfg, 50, 0).await?;
    if cfg.json_output {
        return handle_job_list(cfg, jobs, "Embed");
    }

    println!("{}", primary("Embed Jobs"));
    if jobs.is_empty() {
        println!("  {}", muted("No embed jobs found."));
        return Ok(());
    }

    // Empty map: URLs and paths display as-is; UUID-based paths show parent/markdown.
    let empty_crawl_map = std::collections::HashMap::new();
    for job in &jobs {
        let target = display_embed_input(&job.input_text, &empty_crawl_map);
        let metrics = embed_metrics_suffix(&job.status, job.result_json.as_ref());
        let collection = collection_from_config(&job.config_json);
        let age = job_runtime_text(
            &job.status,
            job.started_at.as_ref(),
            job.finished_at.as_ref(),
            &job.updated_at,
        );
        let collection_str = collection
            .map(|c| format!("{}{}", subtle(" | "), accent(c)))
            .unwrap_or_default();
        let label = status_label(&job.status);
        let prefix = if label.is_empty() {
            format!("  {} ", symbol_for_status(&job.status))
        } else {
            format!("  {} {} ", symbol_for_status(&job.status), label)
        };
        let age_str = format!("{}{}", subtle(" | "), accent(&age));
        println!(
            "{}{}{}{}{} {} {}",
            prefix,
            primary(&target),
            metrics,
            collection_str,
            age_str,
            subtle("|"),
            muted(&job.id.to_string()),
        );
        if let Some(err) = format_error(job.error_text.as_deref()) {
            let err_line = error(&format!("↳ {err}"));
            println!("       {err_line}");
        }
    }
    Ok(())
}

async fn handle_embed_cleanup(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let removed = embed_service::embed_cleanup(cfg).await?;
    handle_job_cleanup(cfg, removed, "embed")
}

async fn handle_embed_clear(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if !confirm_destructive(cfg, "Clear all embed jobs and purge embed queue?")? {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::json!({"removed": 0, "queue_purged": false})
            );
        } else {
            println!("{} aborted", symbol_for_status("canceled"));
        }
        return Ok(());
    }

    let removed = embed_service::embed_clear(cfg).await?;
    handle_job_clear(cfg, removed, "embed")
}

async fn handle_embed_recover(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let reclaimed = embed_service::embed_recover(cfg).await?;
    handle_job_recover(cfg, reclaimed, "embed")
}

fn resolve_embed_input(cfg: &Config) -> String {
    cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    })
}

async fn enqueue_embed_job(cfg: &Config, input: &str) -> Result<(), Box<dyn Error>> {
    // Route through the services layer; the service resolves input from cfg.positional
    // using the same fallback logic, so we temporarily set positional to the resolved input.
    let mut derived = cfg.clone();
    derived.positional = vec![input.to_string()];
    let result = embed_service::embed_start(&derived, None).await?;
    let job_id = result.job_id;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"job_id": job_id, "status": "pending", "source": "rust"})
        );
    } else {
        println!("  {} {}", primary("Embed Job"), accent(&job_id));
        println!("  {}", muted(&format!("Input: {input}")));
        println!("Job ID: {job_id}");
    }
    Ok(())
}
