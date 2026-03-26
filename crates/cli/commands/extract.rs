use crate::crates::cli::commands::CommandFuture;
use crate::crates::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status, handle_worker_mode, parse_urls,
};
use crate::crates::core::config::Config;
use crate::crates::core::content::{
    DeterministicExtractionEngine, ExtractWebConfig, run_extract_with_engine,
};
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::core::ui::{accent, confirm_destructive, muted, primary, symbol_for_status};
use crate::crates::jobs::backend::JobKind;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::extract as extract_service;
use crate::crates::services::jobs as job_service;
use crate::crates::services::types::StartDisposition;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::error::Error;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub fn run_extract<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if maybe_handle_extract_subcommand(cfg).await? {
            return Ok(());
        }

        let urls = parse_urls(cfg);
        if urls.is_empty() {
            return Err(anyhow::anyhow!(
                "extract requires at least one URL (positional or --urls)"
            )
            .into());
        }
        log_info(&format!(
            "command=extract urls={} wait={}",
            urls.len(),
            cfg.wait
        ));
        let prompt = require_extract_prompt(cfg)?;

        if !cfg.wait {
            let result = enqueue_extract_job(cfg, &urls, prompt, service_context).await;
            if result.is_ok() {
                log_info(&format!(
                    "job_enqueued command=extract queue={}",
                    cfg.extract_queue
                ));
            }
            return result;
        }

        run_extract_sync(cfg, urls, &prompt).await
    })
}

async fn maybe_handle_extract_subcommand(cfg: &Config) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };

    match subcmd {
        "status" => {
            let id = parse_extract_job_id(cfg, "status")?;
            let job = job_service::job_status(cfg, JobKind::Extract, id).await?;
            handle_job_status(cfg, job, id, "Extract")?;
        }
        "cancel" => {
            let id = parse_extract_job_id(cfg, "cancel")?;
            let canceled = job_service::cancel_job(cfg, JobKind::Extract, id).await?;
            handle_job_cancel(cfg, id, canceled, "extract")?;
        }
        "errors" => {
            let id = parse_extract_job_id(cfg, "errors")?;
            let job = job_service::job_status(cfg, JobKind::Extract, id).await?;
            handle_job_errors(cfg, job, id, "extract")?;
        }
        "list" => {
            let jobs = job_service::list_jobs(cfg, JobKind::Extract, 50, 0).await?;
            handle_job_list(cfg, jobs, "Extract")?;
        }
        "cleanup" => {
            let removed = job_service::cleanup_jobs(cfg, JobKind::Extract).await?;
            handle_job_cleanup(cfg, removed, "extract")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all extract jobs and purge extract queue?")? {
                let removed = job_service::clear_jobs(cfg, JobKind::Extract).await?;
                handle_job_clear(cfg, removed, "extract")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => handle_worker_mode(job_service::run_worker(cfg, JobKind::Extract).await?)?,
        "recover" => {
            let reclaimed = job_service::recover_jobs(cfg, JobKind::Extract).await?;
            handle_job_recover(cfg, reclaimed, "extract")?;
        }
        _ => return Ok(false),
    }

    Ok(true)
}

fn parse_extract_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("extract {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn require_extract_prompt(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.query
        .as_ref()
        .ok_or("extract requires --query <prompt>")
        .map(|v| v.to_string())
        .map_err(Into::into)
}

async fn enqueue_extract_job(
    cfg: &Config,
    urls: &[String],
    prompt: String,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome =
        extract_service::extract_start_with_context(cfg, urls, Some(prompt), service_context, None)
            .await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"job_id": job_id, "status": status})
        );
    } else {
        println!("  {} {}", primary("Extract Job"), accent(&job_id));
        println!("  {}", muted(&format!("Status: {status}")));
        println!("Job ID: {job_id}");
    }
    Ok(())
}

#[derive(Default)]
struct ExtractAggregation {
    runs: Vec<serde_json::Value>,
    pages_visited: usize,
    pages_with_data: usize,
    deterministic_pages: usize,
    llm_fallback_pages: usize,
    llm_requests: usize,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
    parser_hits: serde_json::Map<String, serde_json::Value>,
    total_items: usize,
}

async fn run_extract_sync(
    cfg: &Config,
    urls: Vec<String>,
    prompt: &str,
) -> Result<(), Box<dyn Error>> {
    let extract_start = std::time::Instant::now();
    let items_path = cfg.output_dir.join("extract-items.ndjson");
    if let Some(parent) = items_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut items_file = tokio::fs::File::create(&items_path).await?;

    let aggregated = execute_extract_runs(cfg, &urls, prompt, &mut items_file).await?;
    let summary = build_extract_summary(cfg, &urls, prompt, &aggregated)?;
    let summary_path = write_extract_summary(cfg, &summary).await?;

    emit_extract_output(cfg, &summary, &summary_path, &items_path)?;
    log_done(&format!(
        "command=extract complete items={} duration_ms={}",
        aggregated.total_items,
        extract_start.elapsed().as_millis()
    ));
    Ok(())
}

async fn execute_extract_runs(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
    items_file: &mut tokio::fs::File,
) -> Result<ExtractAggregation, Box<dyn Error>> {
    let engine = Arc::new(DeterministicExtractionEngine::with_default_parsers());
    let max_pages = cfg.max_pages;
    let openai_base_url_top = cfg.openai_base_url.clone();
    let openai_api_key_top = cfg.openai_api_key.clone();
    let openai_model_top = cfg.openai_model.clone();

    let custom_headers = cfg.custom_headers.clone();

    let mut pending_runs = FuturesUnordered::new();
    for url in urls.iter().cloned() {
        let engine = Arc::clone(&engine);
        let wcfg = ExtractWebConfig {
            start_url: url.clone(),
            prompt: prompt.to_string(),
            limit: max_pages,
            openai_base_url: openai_base_url_top.clone(),
            openai_api_key: openai_api_key_top.clone(),
            openai_model: openai_model_top.clone(),
            acp_adapter_cmd: cfg.acp_adapter_cmd.clone(),
            acp_adapter_args: cfg.acp_adapter_args.clone(),
            custom_headers: custom_headers.clone(),
            render_mode: cfg.render_mode,
            chrome_remote_url: cfg.chrome_remote_url.clone(),
            chrome_stealth: cfg.chrome_stealth,
            chrome_anti_bot: cfg.chrome_anti_bot,
            chrome_intercept: cfg.chrome_intercept,
            bypass_csp: cfg.bypass_csp,
            accept_invalid_certs: cfg.accept_invalid_certs,
            request_timeout_ms: cfg.request_timeout_ms,
            fetch_retries: cfg.fetch_retries,
            user_agent: cfg.chrome_user_agent.clone(),
            chrome_network_idle_timeout_secs: cfg.chrome_network_idle_timeout_secs,
        };
        pending_runs.push(async move {
            let run = run_extract_with_engine(wcfg, engine).await;
            (url, run)
        });
    }

    let mut aggregated = ExtractAggregation::default();
    while let Some((_url, run_result)) = pending_runs.next().await {
        let run = run_result?;
        aggregated.pages_visited += run.pages_visited;
        aggregated.pages_with_data += run.pages_with_data;
        aggregated.deterministic_pages += run.metrics.deterministic_pages;
        aggregated.llm_fallback_pages += run.metrics.llm_fallback_pages;
        aggregated.llm_requests += run.metrics.llm_requests;
        aggregated.prompt_tokens += run.metrics.prompt_tokens;
        aggregated.completion_tokens += run.metrics.completion_tokens;
        aggregated.total_tokens += run.metrics.total_tokens;
        aggregated.estimated_cost_usd += run.metrics.estimated_cost_usd;
        for (name, count) in &run.parser_hits {
            let current = aggregated
                .parser_hits
                .get(name.as_str())
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            aggregated
                .parser_hits
                .insert(name.clone(), serde_json::json!(current + *count as u64));
        }

        aggregated.total_items += run.results.len();

        for item in &run.results {
            let mut line = serde_json::to_string(item)?;
            line.push('\n');
            items_file.write_all(line.as_bytes()).await?;
        }

        aggregated.runs.push(serde_json::json!({
            "url": run.start_url,
            "pages_visited": run.pages_visited,
            "pages_with_data": run.pages_with_data,
            "deterministic_pages": run.metrics.deterministic_pages,
            "llm_fallback_pages": run.metrics.llm_fallback_pages,
            "llm_requests": run.metrics.llm_requests,
            "prompt_tokens": run.metrics.prompt_tokens,
            "completion_tokens": run.metrics.completion_tokens,
            "total_tokens": run.metrics.total_tokens,
            "estimated_cost_usd": run.metrics.estimated_cost_usd,
            "parser_hits": run.parser_hits,
            "total_items": run.results.len(),
        }));
    }

    items_file.flush().await?;
    Ok(aggregated)
}

fn build_extract_summary(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
    aggregated: &ExtractAggregation,
) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::json!({
        "urls": urls,
        "prompt": prompt,
        "model": cfg.openai_model,
        "pages_visited": aggregated.pages_visited,
        "pages_with_data": aggregated.pages_with_data,
        "deterministic_pages": aggregated.deterministic_pages,
        "llm_fallback_pages": aggregated.llm_fallback_pages,
        "llm_requests": aggregated.llm_requests,
        "prompt_tokens": aggregated.prompt_tokens,
        "completion_tokens": aggregated.completion_tokens,
        "total_tokens": aggregated.total_tokens,
        "estimated_cost_usd": aggregated.estimated_cost_usd,
        "parser_hits": aggregated.parser_hits,
        "total_items": aggregated.total_items,
        "runs": aggregated.runs,
    }))
}

async fn write_extract_summary(
    cfg: &Config,
    summary: &serde_json::Value,
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let summary_path = cfg
        .output_path
        .clone()
        .unwrap_or_else(|| cfg.output_dir.join("extract-summary.json"));
    if let Some(parent) = summary_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&summary_path, serde_json::to_string_pretty(summary)?).await?;
    Ok(summary_path)
}

fn emit_extract_output(
    cfg: &Config,
    summary: &serde_json::Value,
    summary_path: &std::path::Path,
    items_path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(summary)?);
        return Ok(());
    }

    println!("{}", primary("Extract Results"));
    println!("  {} {}", muted("Pages visited:"), summary["pages_visited"]);
    println!(
        "  {} {}",
        muted("Pages with data:"),
        summary["pages_with_data"]
    );
    println!(
        "  {} {}",
        muted("Deterministic pages:"),
        summary["deterministic_pages"]
    );
    println!(
        "  {} {}",
        muted("LLM fallback pages:"),
        summary["llm_fallback_pages"]
    );
    println!("  {} {}", muted("LLM requests:"), summary["llm_requests"]);
    println!("  {} {}", muted("Total tokens:"), summary["total_tokens"]);
    println!(
        "  {} {:.6}",
        muted("Estimated cost (USD):"),
        summary["estimated_cost_usd"].as_f64().unwrap_or(0.0)
    );
    println!("  {} {}", muted("Total items:"), summary["total_items"]);
    println!("  {} {}", muted("Summary saved:"), summary_path.display());
    println!("  {} {}", muted("Items saved:"), items_path.display());
    Ok(())
}
