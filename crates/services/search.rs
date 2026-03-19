use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::services::acp_llm::{self, AcpCompletionRequest};
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    ResearchResult, SearchOptions, SearchResult, ServiceTimeRange,
};
use spider_agent::{Agent, SearchOptions as SpiderSearchOptions, TimeRange, TokenUsage};
use std::error::Error;
use std::time::Instant;
use tokio::sync::mpsc;

/// Convert a [`ServiceTimeRange`] to the `spider_agent` crate's [`TimeRange`].
///
/// Private — callers use the typed service options, not spider_agent types directly.
fn to_spider_time_range(tr: ServiceTimeRange) -> TimeRange {
    match tr {
        ServiceTimeRange::Day => TimeRange::Day,
        ServiceTimeRange::Week => TimeRange::Week,
        ServiceTimeRange::Month => TimeRange::Month,
        ServiceTimeRange::Year => TimeRange::Year,
    }
}

/// Execute a Tavily web search and return raw JSON result items.
///
/// This is the core search implementation — validates the API key, builds the
/// spider_agent query, and maps results to a flat `Vec<serde_json::Value>`.
pub async fn search_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err("search requires TAVILY_API_KEY — set it in .env".into());
    }
    let mut search_opts = SpiderSearchOptions::new().with_limit((limit + offset).clamp(1, 100));
    if let Some(tr) = time_range {
        search_opts = search_opts.with_time_range(tr);
    }
    let agent = Agent::builder()
        .with_search_tavily(&cfg.tavily_api_key)
        .build()?;
    let results = agent.search_with_options(query, search_opts).await?;
    Ok(results
        .results
        .iter()
        .skip(offset)
        .take(limit)
        .map(|r| {
            serde_json::json!({
                "position": r.position,
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
            })
        })
        .collect::<Vec<_>>())
}

/// Execute a Tavily AI research query with LLM synthesis.
///
/// Validates config, runs a Tavily search, extracts content snippets, and
/// synthesizes a summary via the configured LLM endpoint. Returns the full
/// research payload as a JSON value.
pub async fn research_payload(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let started = Instant::now();
    if cfg.tavily_api_key.is_empty() {
        return Err("research requires TAVILY_API_KEY — set it in .env".into());
    }
    if cfg
        .acp_adapter_cmd
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err("research requires AXON_ACP_ADAPTER_CMD — set it in .env".into());
    }

    let agent = Agent::builder()
        .with_search_tavily(&cfg.tavily_api_key)
        .build()?;

    // Step 1: search — Tavily returns URLs + content excerpts
    let mut search_options = SpiderSearchOptions::new().with_limit((limit + offset).clamp(1, 100));
    if let Some(tr) = time_range {
        search_options = search_options.with_time_range(tr);
    }
    let search_results = agent.search_with_options(query, search_options).await?;

    // Step 2: use Tavily's content excerpts directly — skip redundant fetch+extract
    let extractions: Vec<serde_json::Value> = search_results
        .results
        .iter()
        .skip(offset)
        .take(limit)
        .map(|r| {
            serde_json::json!({
                "url": r.url,
                "title": r.title,
                "extracted": r.snippet.as_deref().unwrap_or(""),
            })
        })
        .collect();

    // Step 3: synthesize — one LLM call over the snippets
    let (summary, usage) = synthesize(query, &extractions, cfg).await;

    let search_results_json = search_results
        .results
        .iter()
        .skip(offset)
        .take(limit)
        .map(|r| {
            serde_json::json!({
                "position": r.position,
                "title": r.title,
                "url": r.url,
                "snippet": r.snippet,
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "query": query,
        "limit": limit,
        "offset": offset,
        "search_results": search_results_json,
        "extractions": extractions,
        "summary": summary,
        "usage": {
            "prompt_tokens": usage.prompt_tokens,
            "completion_tokens": usage.completion_tokens,
            "total_tokens": usage.total_tokens,
        },
        "timing_ms": {
            "total": started.elapsed().as_millis(),
        },
    }))
}

/// Synthesize extracted content into a coherent summary via LLM.
async fn synthesize(
    query: &str,
    extractions: &[serde_json::Value],
    cfg: &Config,
) -> (Option<String>, TokenUsage) {
    if extractions.is_empty() {
        return (None, TokenUsage::default());
    }

    let mut context = String::new();
    for (i, e) in extractions.iter().enumerate() {
        context.push_str(&format!(
            "\n\nSource {} ({}): {}\n{}",
            i + 1,
            e["url"].as_str().unwrap_or(""),
            e["title"].as_str().unwrap_or(""),
            e["extracted"].as_str().unwrap_or(""),
        ));
    }

    let mut req = AcpCompletionRequest::new(format!(
        "Topic: {query}\n\nSources:{context}\n\nProvide a comprehensive summary of the findings, citing sources where appropriate. Return as JSON with a 'summary' field."
    ))
    .system_prompt(
        "You are a research synthesis assistant. Summarize the findings from multiple sources into a coherent response.",
    );
    if !cfg.openai_model.trim().is_empty() {
        req = req.model(cfg.openai_model.clone());
    }

    let cfg_for_completion = cfg.clone();
    match tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| format!("failed to build research ACP runtime: {err}"))?;
        rt.block_on(acp_llm::complete_text(&cfg_for_completion, req))
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| format!("failed to join research ACP task: {err}"))
    {
        Ok(Ok(response)) => {
            let summary = serde_json::from_str::<serde_json::Value>(&response.text)
                .ok()
                .and_then(|v| {
                    v.get("summary")
                        .and_then(|s| s.as_str())
                        .map(str::to_string)
                })
                .unwrap_or(response.text);
            let usage = response
                .usage
                .map(|u| TokenUsage {
                    prompt_tokens: u32::try_from(u.prompt_tokens).unwrap_or(u32::MAX),
                    completion_tokens: u32::try_from(u.completion_tokens).unwrap_or(u32::MAX),
                    total_tokens: u32::try_from(u.total_tokens).unwrap_or(u32::MAX),
                })
                .unwrap_or_default();
            (Some(summary), usage)
        }
        Ok(Err(e)) => {
            log_warn(&format!("synthesis failed: {e}"));
            (None, TokenUsage::default())
        }
        Err(e) => {
            log_warn(&format!("synthesis failed: {e}"));
            (None, TokenUsage::default())
        }
    }
}

/// Map a `Vec<serde_json::Value>` of raw search items into a typed [`SearchResult`].
///
/// This is a pure function — no network required. Tests call it with JSON literals.
pub fn map_search_results(results: Vec<serde_json::Value>) -> SearchResult {
    SearchResult { results }
}

/// Map a raw JSON payload into a typed [`ResearchResult`].
///
/// This is a pure function — no network required. Tests call it with JSON literals.
pub fn map_research_payload(payload: serde_json::Value) -> ResearchResult {
    ResearchResult { payload }
}

/// Run a web search via Tavily and return a typed [`SearchResult`].
///
/// Delegates to [`search_results`] from the CLI commands layer. Emits log events
/// when a `tx` sender is provided.
pub async fn search(
    cfg: &Config,
    query: &str,
    opts: SearchOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SearchResult, Box<dyn Error>> {
    search_batch(cfg, &[query], opts, tx).await
}

/// Run multiple Tavily searches in sequence and return merged results.
///
/// Each query is searched independently; results are flattened in order.
/// Emits log events when a `tx` sender is provided.
pub async fn search_batch(
    cfg: &Config,
    queries: &[&str],
    opts: SearchOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SearchResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("starting search: {}", queries.join(", ")),
        },
    )
    .await;

    let time_range = opts.time_range.map(to_spider_time_range);
    let mut all: Vec<serde_json::Value> = Vec::new();
    for query in queries {
        let mut raw =
            search_results(cfg, query, opts.limit, opts.offset, time_range.clone()).await?;
        all.append(&mut raw);
    }

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("search complete: {} results", all.len()),
        },
    )
    .await;

    Ok(map_search_results(all))
}

/// Run a Tavily AI research query with LLM synthesis and return a typed [`ResearchResult`].
///
/// Delegates to [`research_payload`] from the CLI commands layer. Emits log events
/// when a `tx` sender is provided.
pub async fn research(
    cfg: &Config,
    query: &str,
    opts: SearchOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ResearchResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("starting research: {query}"),
        },
    )
    .await;

    let time_range = opts.time_range.map(to_spider_time_range);
    let payload = research_payload(cfg, query, opts.limit, opts.offset, time_range).await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "research complete".to_string(),
        },
    )
    .await;

    Ok(map_research_payload(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── to_spider_time_range ──────────────────────────────────────────────────

    #[test]
    fn time_range_all_variants_map_correctly() {
        // TimeRange is already in scope via `use spider_agent::TimeRange` pulled
        // in through `use super::*`.
        assert_eq!(to_spider_time_range(ServiceTimeRange::Day), TimeRange::Day);
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Week),
            TimeRange::Week
        );
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Month),
            TimeRange::Month
        );
        assert_eq!(
            to_spider_time_range(ServiceTimeRange::Year),
            TimeRange::Year
        );
    }

    // ── map_search_results ────────────────────────────────────────────────────

    #[test]
    fn map_search_results_empty_vec() {
        let result = map_search_results(vec![]);
        assert!(result.results.is_empty());
    }

    #[test]
    fn map_search_results_nonempty() {
        let item = json!({"title": "Axon docs", "url": "https://example.com"});
        let result = map_search_results(vec![item.clone()]);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0], item);
    }

    // ── map_research_payload ──────────────────────────────────────────────────

    #[test]
    fn map_research_payload_stores_value() {
        let value = json!({"answer": "42", "sources": []});
        let result = map_research_payload(value.clone());
        assert_eq!(result.payload, value);
    }

    // ── search_batch (pure path: empty query slice) ───────────────────────────
    //
    // With zero queries the loop body never executes and no network call is
    // made, so this runs without a live Tavily key.  The two emit() calls still
    // fire but they are no-ops when `tx` is None.

    #[tokio::test]
    async fn search_batch_empty_queries_returns_empty() {
        let cfg = Config::default();
        let result = search_batch(
            &cfg,
            &[],
            SearchOptions {
                limit: 10,
                offset: 0,
                time_range: None,
            },
            None,
        )
        .await
        .expect("search_batch with empty queries should not fail");
        assert!(result.results.is_empty());
    }
}
