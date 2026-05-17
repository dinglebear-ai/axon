use super::deterministic::{
    DeterministicExtractionEngine, ExtractRun, FallbackResponse, extract_items_fallback,
};
use super::{ExtractionMetrics, to_markdown};
use crate::core::config::RenderMode;
use crate::core::http::{http_client, parse_custom_headers, ssrf_blacklist_patterns, validate_url};
use crate::core::logging::log_warn;
use spider::website::Website;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

mod chrome;
use chrome::run_single_url_extract_chrome;

const FALLBACK_CONCURRENCY_LIMIT: usize = 4;

/// Configuration bundle for `run_extract_with_engine`.
///
/// Replaces the previous 7-param function signature with a single struct,
/// making it easy to add new fields (e.g. `custom_headers`) without churn.
pub struct ExtractWebConfig {
    pub start_url: String,
    pub prompt: String,
    pub limit: u32,
    pub llm_backend: crate::services::llm_backend::LlmBackendConfig,
    /// Custom HTTP headers in `"Key: Value"` format, passed through to spider.
    pub custom_headers: Vec<String>,
    // ── Rendering / Chrome ──────────────────────────────────────────────────
    pub render_mode: RenderMode,
    /// CDP management URL (e.g. `http://axon-chrome:6000`). `None` = no Chrome.
    pub chrome_remote_url: Option<String>,
    pub chrome_stealth: bool,
    pub chrome_anti_bot: bool,
    pub chrome_intercept: bool,
    pub bypass_csp: bool,
    pub accept_invalid_certs: bool,
    pub request_timeout_ms: Option<u64>,
    pub fetch_retries: usize,
    /// User-Agent string (from `AXON_CHROME_USER_AGENT`).
    pub user_agent: Option<String>,
    /// Seconds to wait for network idle after initial page load (Chrome path only).
    /// Passed to `with_wait_for_idle_network0`. Maps to `cfg.chrome_network_idle_timeout_secs`.
    pub chrome_network_idle_timeout_secs: u64,
}

struct FallbackConfig {
    llm_backend: crate::services::llm_backend::LlmBackendConfig,
    prompt_text: String,
    has_fallback: bool,
}

struct PageCollectResult {
    results: Vec<serde_json::Value>,
    pages_visited: usize,
    pages_with_data: usize,
    metrics: ExtractionMetrics,
    parser_hits: HashMap<String, usize>,
}

fn apply_deterministic_results(
    deterministic: super::deterministic::PageExtraction,
    metrics: &mut ExtractionMetrics,
    pages_with_data: &mut usize,
    all_results: &mut Vec<serde_json::Value>,
    parser_hits: &mut HashMap<String, usize>,
) -> bool {
    if deterministic.items.is_empty() {
        return false;
    }
    metrics.deterministic_pages += 1;
    *pages_with_data += 1;
    all_results.extend(deterministic.items);
    for hit in deterministic.parser_hits {
        *parser_hits.entry(hit).or_insert(0) += 1;
    }
    true
}

fn queue_fallback_extraction(
    fallback_tasks: &mut JoinSet<(String, Result<FallbackResponse, String>)>,
    fallback_limiter: Arc<Semaphore>,
    client: reqwest::Client,
    cfg: &FallbackConfig,
    page_url: String,
    html: String,
) {
    let llm_backend_c = cfg.llm_backend.clone();
    let prompt_c = cfg.prompt_text.clone();
    fallback_tasks.spawn(async move {
        // Run CPU-bound HTML→markdown conversion via spawn_blocking BEFORE
        // acquiring the semaphore permit. This prevents blocking the Tokio
        // executor thread and avoids inflating permit hold-time (only the
        // downstream LLM call needs the permit).
        let markdown = match tokio::task::spawn_blocking(move || to_markdown(&html, None)).await {
            Ok(md) => md,
            Err(e) => {
                return (
                    page_url,
                    Err(format!("markdown conversion join error: {e}")),
                );
            }
        };
        let _permit = match fallback_limiter.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                return (page_url, Err("fallback limiter closed".to_string()));
            }
        };
        let res = extract_items_fallback(&client, llm_backend_c, &prompt_c, &page_url, &markdown)
            .await
            .map_err(|e| e.to_string());
        (page_url, res)
    });
}

async fn collect_page_results(
    mut rx: tokio::sync::broadcast::Receiver<spider::page::Page>,
    client: reqwest::Client,
    engine: Arc<DeterministicExtractionEngine>,
    cfg: FallbackConfig,
) -> PageCollectResult {
    let mut all_results: Vec<serde_json::Value> = vec![];
    let mut pages_visited = 0usize;
    let mut pages_with_data = 0usize;
    let mut metrics = ExtractionMetrics::default();
    let mut parser_hits: HashMap<String, usize> = HashMap::new();
    let fallback_limiter = Arc::new(Semaphore::new(FALLBACK_CONCURRENCY_LIMIT));
    let mut fallback_tasks: JoinSet<(String, Result<FallbackResponse, String>)> = JoinSet::new();

    loop {
        let page = match rx.recv().await {
            Ok(page) => page,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log_warn(&format!(
                    "broadcast receiver lagged, skipped {n} pages — consider increasing buffer"
                ));
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        pages_visited += 1;
        let page_url = page.get_url().to_string();
        let html = page.get_html();
        if html.is_empty() {
            continue;
        }
        let deterministic = engine.extract(&page_url, &html);
        if apply_deterministic_results(
            deterministic,
            &mut metrics,
            &mut pages_with_data,
            &mut all_results,
            &mut parser_hits,
        ) {
            continue;
        }
        if !cfg.has_fallback {
            continue;
        }
        metrics.llm_fallback_pages += 1;
        metrics.llm_requests += 1;
        queue_fallback_extraction(
            &mut fallback_tasks,
            Arc::clone(&fallback_limiter),
            client.clone(),
            &cfg,
            page_url,
            html,
        );
        while let Some(joined) = fallback_tasks.try_join_next() {
            drain_fallback_result(joined, &mut pages_with_data, &mut all_results, &mut metrics);
        }
    }
    while let Some(joined) = fallback_tasks.join_next().await {
        drain_fallback_result(joined, &mut pages_with_data, &mut all_results, &mut metrics);
    }
    PageCollectResult {
        results: all_results,
        pages_visited,
        pages_with_data,
        metrics,
        parser_hits,
    }
}

fn drain_fallback_result(
    joined: Result<(String, Result<FallbackResponse, String>), tokio::task::JoinError>,
    pages_with_data: &mut usize,
    all_results: &mut Vec<serde_json::Value>,
    metrics: &mut ExtractionMetrics,
) {
    match joined {
        Ok((url, Ok(fallback))) => {
            metrics.prompt_tokens += fallback.prompt_tokens;
            metrics.completion_tokens += fallback.completion_tokens;
            metrics.total_tokens += fallback.total_tokens;
            if fallback.items.is_empty() {
                log_warn(&format!("fallback extraction produced no items for {url}"));
            } else {
                *pages_with_data += 1;
                all_results.extend(fallback.items);
            }
        }
        Ok((url, Err(err))) => {
            metrics.llm_fallback_failures += 1;
            log_warn(&format!("fallback extraction failed for {url}: {err}"));
        }
        Err(err) => {
            metrics.llm_fallback_failures += 1;
            log_warn(&format!("fallback extraction task join error: {err}"));
        }
    }
}

fn all_fallback_attempts_failed(
    metrics: &ExtractionMetrics,
    results: &[serde_json::Value],
) -> bool {
    metrics.llm_fallback_pages > 0
        && metrics.llm_fallback_failures == metrics.llm_fallback_pages
        && results.is_empty()
}

/// Fetch a single URL directly with reqwest and extract structured data from it.
///
/// Bypasses spider entirely — spider normalises deep URL paths to the domain
/// root (e.g. `en.wikipedia.org/wiki/Rust` → `en.wikipedia.org/`), which causes
/// extraction to run against the wrong page. For single-URL extraction we always
/// want the exact page the caller requested.
async fn run_single_url_extract(
    url: &str,
    client: reqwest::Client,
    engine: Arc<DeterministicExtractionEngine>,
    cfg: FallbackConfig,
    custom_headers: &[String],
    user_agent: Option<&str>,
) -> Result<ExtractRun, Box<dyn Error>> {
    let mut req = client.get(url);
    let header_map = parse_custom_headers(custom_headers);
    if !header_map.is_empty() {
        req = req.headers(header_map);
    }
    if let Some(ua) = user_agent {
        req = req.header(reqwest::header::USER_AGENT, ua);
    }
    let html = req.send().await?.error_for_status()?.text().await?;

    let mut metrics = ExtractionMetrics::default();
    let mut pages_with_data = 0usize;
    let mut all_results = Vec::new();
    let mut parser_hits = HashMap::new();

    let det = engine.extract(url, &html);
    let det_matched = apply_deterministic_results(
        det,
        &mut metrics,
        &mut pages_with_data,
        &mut all_results,
        &mut parser_hits,
    );

    if !det_matched && cfg.has_fallback {
        metrics.llm_fallback_pages += 1;
        metrics.llm_requests += 1;
        let markdown = to_markdown(&html, None);
        match extract_items_fallback(
            &client,
            cfg.llm_backend.clone(),
            &cfg.prompt_text,
            url,
            &markdown,
        )
        .await
        {
            Ok(fallback) => {
                metrics.prompt_tokens += fallback.prompt_tokens;
                metrics.completion_tokens += fallback.completion_tokens;
                metrics.total_tokens += fallback.total_tokens;
                if !fallback.items.is_empty() {
                    pages_with_data += 1;
                    all_results.extend(fallback.items);
                } else {
                    log_warn(&format!("fallback extraction produced no items for {url}"));
                }
            }
            Err(e) => {
                return Err(format!("fallback extraction failed for {url}: {e}").into());
            }
        }
    }

    Ok(ExtractRun {
        start_url: url.to_string(),
        pages_visited: 1,
        pages_with_data,
        results: all_results,
        metrics,
        parser_hits,
    })
}

pub async fn run_extract_with_engine(
    wcfg: ExtractWebConfig,
    engine: Arc<DeterministicExtractionEngine>,
) -> Result<ExtractRun, Box<dyn Error>> {
    let has_fallback = !wcfg.prompt.trim().is_empty();

    validate_url(&wcfg.start_url)?;

    // Clone start_url before partial moves into fallback_cfg.
    let start_url = wcfg.start_url.clone();

    let fallback_cfg = FallbackConfig {
        llm_backend: wcfg.llm_backend.clone(),
        prompt_text: wcfg.prompt.clone(),
        has_fallback,
    };

    // Single-page: bypass spider to fetch the exact URL. Spider normalises deep
    // paths to the domain root (Website::new strips the path component), so
    // requests for /wiki/Rust or /recipe/12345 land on the site homepage instead.
    // For Chrome mode, we use spider with limit=1 to get stealth + fingerprint
    // patches. For HTTP mode, plain reqwest fetches the exact URL directly.
    if wcfg.limit == 1 {
        return match wcfg.render_mode {
            RenderMode::Chrome => {
                run_single_url_extract_chrome(&start_url, engine, &wcfg, fallback_cfg).await
            }
            _ => {
                run_single_url_extract(
                    &start_url,
                    http_client()?.clone(),
                    engine,
                    fallback_cfg,
                    &wcfg.custom_headers,
                    wcfg.user_agent.as_deref(),
                )
                .await
            }
        };
    }

    let ssrf_patterns: Vec<spider::compact_str::CompactString> = ssrf_blacklist_patterns()
        .iter()
        .copied()
        .map(Into::into)
        .collect();
    let mut website = Website::new(&wcfg.start_url);
    website.with_limit(wcfg.limit);
    website.with_blacklist_url(Some(ssrf_patterns));
    // Wire custom headers so `--header` applies to extract crawls too.
    if !wcfg.custom_headers.is_empty() {
        let map = parse_custom_headers(&wcfg.custom_headers);
        if !map.is_empty() {
            website.with_headers(Some(map));
        }
    }
    // Wire user-agent so HTTP extract crawls use the same UA as scrape/crawl.
    if let Some(ua) = wcfg.user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
    let mut website = website.build().map_err(|_| "build website")?;

    let rx = website.subscribe(16);
    let collect = tokio::spawn(collect_page_results(
        rx,
        http_client()?.clone(),
        Arc::clone(&engine),
        fallback_cfg,
    ));

    website.crawl_raw().await;
    website.unsubscribe();

    let PageCollectResult {
        results,
        pages_visited,
        pages_with_data,
        metrics,
        parser_hits,
    } = collect.await?;
    if all_fallback_attempts_failed(&metrics, &results) {
        return Err("all LLM fallback extraction attempts failed".into());
    }
    Ok(ExtractRun {
        start_url: wcfg.start_url,
        pages_visited,
        pages_with_data,
        results,
        metrics,
        parser_hits,
    })
}
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
