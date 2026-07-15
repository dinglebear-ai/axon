//! Synchronous structured extraction over one or more URLs.
//!
//! Drives the deterministic extraction engine (LLM fallback) per URL, writes an
//! NDJSON item stream + a JSON summary, and returns an `ExtractSyncResult`.
//! Relocated from the removed `axon-extract` crate (Phase 12 clean break) — it
//! never depended on anything but `axon-core`/`axon-api`/`axon-llm`, so it
//! moves here verbatim rather than needing a new crate of its own.

use axon_api::job_dto::ExtractSyncResult;
use axon_core::artifacts::write_configured_output;
use axon_core::config::Config;
use axon_core::content::{
    DeterministicExtractionEngine, ExtractRun, ExtractWebConfig, ExtractionMetrics,
    run_extract_with_engine,
};
use axon_core::http::axon_ua;
use axon_core::logging::log_done;
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};
use axon_extract::{ScrapedDoc, VerticalContext, dispatch_by_url};
use futures_util::StreamExt;
use futures_util::stream;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

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
            let cfg = cfg.clone();
            async move { run_single_extract(cfg, wcfg, engine).await }
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

async fn run_single_extract(
    cfg: Config,
    wcfg: ExtractWebConfig,
    engine: Arc<DeterministicExtractionEngine>,
) -> Result<ExtractRun, Box<dyn Error>> {
    if cfg.enable_verticals {
        let ctx = VerticalContext::new(Arc::new(cfg));
        match tokio::time::timeout(
            Duration::from_secs(120),
            dispatch_by_url(&wcfg.start_url, &ctx),
        )
        .await
        {
            Ok(Some(Ok(doc))) => return Ok(vertical_doc_to_extract_run(doc)),
            Ok(Some(Err(err))) => {
                axon_core::logging::log_warn(&format!(
                    "vertical extractor failed for {}; falling back to generic extract: {err}",
                    wcfg.start_url
                ));
            }
            Ok(None) => {}
            Err(_) => {
                axon_core::logging::log_warn(&format!(
                    "vertical extractor timed out for {}; falling back to generic extract",
                    wcfg.start_url
                ));
            }
        }
    }
    run_extract_with_engine(wcfg, engine).await
}

fn vertical_doc_to_extract_run(doc: ScrapedDoc) -> ExtractRun {
    let extractor_name = doc.extractor_name;
    let extractor_version = doc.extractor_version;
    let markdown_chars = doc.markdown.chars().count();
    let markdown_excerpt: String = doc.markdown.chars().take(4000).collect();
    let mut item = serde_json::json!({
        "url": doc.url,
        "title": doc.title,
        "extractor_name": extractor_name,
        "extractor_version": extractor_version,
        "kind": "vertical_extraction",
        "markdown_chars": markdown_chars,
        "markdown_excerpt": markdown_excerpt,
        "markdown_excerpt_truncated": markdown_chars > 4000,
        "follow_crawl_urls": doc.follow_crawl_urls,
    });
    if let Some(structured) = doc.structured {
        item["structured"] = structured;
    }
    if let Some(extra) = doc.extra {
        item["extra"] = extra;
    }
    let mut parser_hits = HashMap::new();
    parser_hits.insert(format!("vertical:{extractor_name}"), 1);
    ExtractRun {
        start_url: item["url"].as_str().unwrap_or_default().to_string(),
        pages_visited: 1,
        pages_with_data: 1,
        results: vec![item],
        metrics: ExtractionMetrics {
            deterministic_pages: 1,
            ..ExtractionMetrics::default()
        },
        parser_hits,
    }
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
        llm_backend: axon_llm::LlmBackendConfig::from_config(cfg),
        completer: axon_llm::backend_text_completer(),
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
        "model": axon_llm::configured_model_from_config(cfg),
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
    // Fail-closed redaction boundary: this run summary is an artifact
    // metadata write (per-URL run status/errors), not the scraped body
    // content itself, so it gets the same scrub as job events/reset
    // receipts before it lands on disk.
    let (summary, _redaction_report) =
        DefaultRedactor::new().redact_json(summary.clone(), &RedactionContext::artifact_metadata());
    write_configured_output(
        &cfg.output_dir,
        cfg.output_path.as_deref(),
        "extract-summary.json",
        serde_json::to_string_pretty(&summary)?.as_bytes(),
    )
    .await
    .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    Ok(summary_path)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
