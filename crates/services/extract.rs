//! Service-layer wrappers for extract job lifecycle operations and prompt-aware enqueue helpers.

use crate::crates::core::config::Config;
use crate::crates::core::content::{
    DeterministicExtractionEngine, ExtractWebConfig, run_extract_with_engine,
};
use crate::crates::core::logging::log_done;
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::jobs::extract::start_extract_job;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    ExecutionMode, ExtractJobResult, ExtractStartResult, ExtractSyncResult, JobStartOutcome,
    StartDisposition,
};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::error::Error;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use uuid::Uuid;

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_extract_start_result(job_id: String) -> ExtractStartResult {
    ExtractStartResult { job_id }
}

pub fn map_extract_job_result(payload: serde_json::Value) -> ExtractJobResult {
    ExtractJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn extract_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<ExtractJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Extract, id).await?;
    Ok(job.map(|value| {
        map_extract_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn extract_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<ExtractJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Extract, limit, offset).await?;
    Ok(map_extract_job_result(serde_json::to_value(jobs)?))
}

pub async fn extract_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Extract, id).await
}

pub async fn extract_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Extract).await
}

pub async fn extract_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::run_worker(service_context, JobKind::Extract).await? {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

/// Enqueue an extract job for the given URLs and return its job ID immediately.
/// The extract prompt is read from cfg.query if present.
pub async fn extract_start(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ExtractStartResult, Box<dyn Error>> {
    extract_start_with_prompt(cfg, urls, cfg.query.clone(), tx).await
}

pub async fn extract_start_with_prompt(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ExtractStartResult, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("extract_start requires at least one URL".into());
    }

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueueing extract job for {} URL(s)", urls.len()),
        },
    )
    .await;

    let job_id = start_extract_job(cfg, urls, prompt).await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("enqueued extract job: {job_id}"),
        },
    )
    .await;

    Ok(map_extract_start_result(job_id.to_string()))
}

pub async fn extract_start_with_context(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<ExtractStartResult>, Box<dyn Error>> {
    if !cfg.lite_mode {
        let result = extract_start_with_prompt(cfg, urls, prompt, tx).await?;
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::Enqueued,
            result,
        });
    }

    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Extract {
            urls: urls.to_vec(),
            config_json: "{}".to_string(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_extract_start_result(job_id.to_string()),
    })
}

// --- Sync extract (--wait true) ---

/// Run extraction synchronously: crawl + extract all URLs inline and write
/// output files. Returns a typed result with the summary JSON, file paths,
/// and aggregate stats.
pub async fn extract_sync(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
) -> Result<ExtractSyncResult, Box<dyn Error>> {
    let extract_start = std::time::Instant::now();
    let items_path = cfg.output_dir.join("extract-items.ndjson");
    if let Some(parent) = items_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut items_file = tokio::fs::File::create(&items_path).await?;

    let agg = execute_extract_runs(cfg, urls, prompt, &mut items_file).await?;
    let summary = build_extract_summary(cfg, urls, prompt, &agg)?;
    let summary_path = write_extract_summary(cfg, &summary).await?;
    let duration_ms = extract_start.elapsed().as_millis();

    log_done(&format!(
        "command=extract complete items={} duration_ms={duration_ms}",
        agg.total_items,
    ));

    Ok(ExtractSyncResult {
        summary,
        summary_path: summary_path.to_string_lossy().into_owned(),
        items_path: items_path.to_string_lossy().into_owned(),
        total_items: agg.total_items,
        duration_ms,
    })
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

async fn execute_extract_runs(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
    items_file: &mut tokio::fs::File,
) -> Result<ExtractAggregation, Box<dyn Error>> {
    let engine = Arc::new(DeterministicExtractionEngine::with_default_parsers());

    let mut pending_runs = FuturesUnordered::new();
    for url in urls.iter().cloned() {
        let engine = Arc::clone(&engine);
        let wcfg = build_extract_web_config(cfg, url.clone(), prompt);
        pending_runs.push(async move {
            let run = run_extract_with_engine(wcfg, engine).await;
            (url, run)
        });
    }

    let mut agg = ExtractAggregation::default();
    while let Some((_url, run_result)) = pending_runs.next().await {
        let run = run_result?;
        accumulate_run(&mut agg, &run);

        for item in &run.results {
            let mut line = serde_json::to_string(item)?;
            line.push('\n');
            items_file.write_all(line.as_bytes()).await?;
        }

        agg.runs.push(serde_json::json!({
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
    Ok(agg)
}

fn accumulate_run(agg: &mut ExtractAggregation, run: &crate::crates::core::content::ExtractRun) {
    agg.pages_visited += run.pages_visited;
    agg.pages_with_data += run.pages_with_data;
    agg.deterministic_pages += run.metrics.deterministic_pages;
    agg.llm_fallback_pages += run.metrics.llm_fallback_pages;
    agg.llm_requests += run.metrics.llm_requests;
    agg.prompt_tokens += run.metrics.prompt_tokens;
    agg.completion_tokens += run.metrics.completion_tokens;
    agg.total_tokens += run.metrics.total_tokens;
    agg.estimated_cost_usd += run.metrics.estimated_cost_usd;
    for (name, count) in &run.parser_hits {
        let current = agg
            .parser_hits
            .get(name.as_str())
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        agg.parser_hits
            .insert(name.clone(), serde_json::json!(current + *count as u64));
    }
    agg.total_items += run.results.len();
}

fn build_extract_web_config(cfg: &Config, url: String, prompt: &str) -> ExtractWebConfig {
    ExtractWebConfig {
        start_url: url,
        prompt: prompt.to_string(),
        limit: cfg.max_pages,
        openai_base_url: cfg.openai_base_url.clone(),
        openai_api_key: cfg.openai_api_key.clone(),
        openai_model: cfg.openai_model.clone(),
        acp_adapter_cmd: cfg.acp_adapter_cmd.clone(),
        acp_adapter_args: cfg.acp_adapter_args.clone(),
        custom_headers: cfg.custom_headers.clone(),
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
    }
}

fn build_extract_summary(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
    agg: &ExtractAggregation,
) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::json!({
        "urls": urls,
        "prompt": prompt,
        "model": cfg.openai_model,
        "pages_visited": agg.pages_visited,
        "pages_with_data": agg.pages_with_data,
        "deterministic_pages": agg.deterministic_pages,
        "llm_fallback_pages": agg.llm_fallback_pages,
        "llm_requests": agg.llm_requests,
        "prompt_tokens": agg.prompt_tokens,
        "completion_tokens": agg.completion_tokens,
        "total_tokens": agg.total_tokens,
        "estimated_cost_usd": agg.estimated_cost_usd,
        "parser_hits": agg.parser_hits,
        "total_items": agg.total_items,
        "runs": agg.runs,
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
