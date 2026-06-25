//! Synchronous structured extraction over one or more URLs.
//!
//! Drives the deterministic extraction engine (LLM fallback) per URL, writes an
//! NDJSON item stream + a JSON summary, and returns an `ExtractSyncResult`. Takes
//! only `cfg` (no `ServiceContext`, no jobs), so it lives here in `axon-extract`
//! and is called by both `services::extract` and the jobs extract runner.

use axon_api::job_dto::ExtractSyncResult;
use axon_core::artifacts::write_configured_output;
use axon_core::config::Config;
use axon_core::content::{
    DeterministicExtractionEngine, ExtractWebConfig, run_extract_with_engine,
};
use axon_core::http::axon_ua;
use axon_core::logging::log_done;
use futures_util::StreamExt;
use futures_util::stream;
use std::error::Error;
use std::io::Write;
use std::sync::Arc;

const MAX_INLINE_EXTRACT_ITEMS: usize = 25;

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
    let mut items_file = std::fs::File::create(&items_path)?;

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
    parser_hits: serde_json::Map<String, serde_json::Value>,
    total_items: usize,
    sample_items: Vec<serde_json::Value>,
}

async fn execute_extract_runs(
    cfg: &Config,
    urls: &[String],
    prompt: &str,
    items_file: &mut std::fs::File,
) -> Result<ExtractAggregation, Box<dyn Error>> {
    let engine = Arc::new(DeterministicExtractionEngine::with_default_parsers());

    let mut pending_runs = stream::iter(urls.iter().cloned())
        .map(|url| {
            let engine = Arc::clone(&engine);
            let wcfg = build_extract_web_config(cfg, url, prompt);
            async move { run_extract_with_engine(wcfg, engine).await }
        })
        .buffer_unordered(16);

    let mut agg = ExtractAggregation::default();
    while let Some(run_result) = pending_runs.next().await {
        let item_lines = {
            let run = run_result?;
            accumulate_run(&mut agg, &run);

            let mut item_lines = Vec::with_capacity(run.results.len());
            for item in &run.results {
                if agg.sample_items.len() < MAX_INLINE_EXTRACT_ITEMS {
                    agg.sample_items.push(item.clone());
                }
                let mut line = serde_json::to_string(item)?;
                line.push('\n');
                item_lines.push(line);
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
                "parser_hits": run.parser_hits,
                "total_items": run.results.len(),
            }));
            item_lines
        };

        for line in item_lines {
            items_file.write_all(line.as_bytes())?;
        }
    }

    items_file.flush()?;
    Ok(agg)
}

fn accumulate_run(agg: &mut ExtractAggregation, run: &axon_core::content::ExtractRun) {
    agg.pages_visited += run.pages_visited;
    agg.pages_with_data += run.pages_with_data;
    agg.deterministic_pages += run.metrics.deterministic_pages;
    agg.llm_fallback_pages += run.metrics.llm_fallback_pages;
    agg.llm_requests += run.metrics.llm_requests;
    agg.prompt_tokens += run.metrics.prompt_tokens;
    agg.completion_tokens += run.metrics.completion_tokens;
    agg.total_tokens += run.metrics.total_tokens;
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
        llm_backend: axon_core::llm::LlmBackendConfig::from_config(cfg),
        custom_headers: cfg.custom_headers.clone(),
        render_mode: cfg.render_mode,
        chrome_remote_url: cfg.chrome_remote_url.clone(),
        bypass_csp: cfg.bypass_csp,
        accept_invalid_certs: cfg.accept_invalid_certs,
        request_timeout_ms: cfg.request_timeout_ms,
        fetch_retries: cfg.fetch_retries,
        user_agent: Some(
            cfg.chrome_user_agent
                .as_deref()
                .unwrap_or_else(|| axon_ua())
                .to_string(),
        ),
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
        "model": axon_core::llm::configured_model_from_config(cfg),
        "pages_visited": agg.pages_visited,
        "pages_with_data": agg.pages_with_data,
        "deterministic_pages": agg.deterministic_pages,
        "llm_fallback_pages": agg.llm_fallback_pages,
        "llm_requests": agg.llm_requests,
        "prompt_tokens": agg.prompt_tokens,
        "completion_tokens": agg.completion_tokens,
        "total_tokens": agg.total_tokens,
        "parser_hits": agg.parser_hits,
        "total_items": agg.total_items,
        "items": agg.sample_items,
        "items_truncated": agg.total_items > agg.sample_items.len(),
        "inline_items_limit": MAX_INLINE_EXTRACT_ITEMS,
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
    write_configured_output(
        &cfg.output_dir,
        cfg.output_path.as_deref(),
        "extract-summary.json",
        serde_json::to_string_pretty(summary)?.as_bytes(),
    )
    .await
    .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    Ok(summary_path)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
