use crate::commands::common::{
    parse_service_time_range, truncate_display_continuation, truncate_display_text,
};
use crate::commands::resolve_input_text;
use axon_core::config::Config;
use axon_core::logging::{log_done, log_info, log_warn};
use axon_core::ui::{muted, primary, print_phase};
use axon_services::context::ServiceContext;
use axon_services::search_crawl::{SearchCrawlJob, SearchCrawlRejection, search_and_crawl};
use axon_services::types::SearchOptions as ServiceSearchOptions;
use serde_json::Value;
use std::error::Error;

const HUMAN_SNIPPET_LIMIT: usize = 240;
const SEARCH_ITEM_INDENT: usize = 3;

pub async fn run_search(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() && cfg.searxng_url.is_empty() {
        return Err(anyhow::anyhow!(
            "search requires AXON_SEARXNG_URL or TAVILY_API_KEY — set one in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }

    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("search requires a query (positional or --query)"))?;

    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=search query_len={}", query.len()));
        print_phase("\u{25d0}", "Searching", &query);
    }

    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };

    let search_start = std::time::Instant::now();
    let result = search_and_crawl(cfg, service_context, &query, opts)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    let duration_ms = search_start.elapsed().as_millis();

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "auto_crawl_status": result.auto_crawl_status,
                "query": query,
                "limit": cfg.search_limit,
                "offset": 0,
                "search_time_range": cfg.search_time_range.as_deref(),
                "results": result.results,
                "crawl_jobs": result.crawl_jobs,
                "crawl_jobs_rejected": result.crawl_rejected,
            }))?
        );
        return Ok(());
    }

    print_search_results(&query, &result.results);
    log_crawl_summary(cfg, &result.crawl_jobs, &result.crawl_rejected);

    if !result.results.is_empty() && result.crawl_jobs.is_empty() {
        let reason = result
            .crawl_rejected
            .first()
            .map(|r| r.reason.as_str())
            .unwrap_or("unknown rejection");
        return Err(anyhow::anyhow!(
            "search completed, but no result URLs were queued for source auto-index; first failure: {reason}"
        )
        .into());
    }

    if !cfg.quiet {
        log_done(&format!(
            "command=search complete query_len={} results={} duration_ms={duration_ms}",
            query.len(),
            result.results.len()
        ));
    }
    Ok(())
}

fn log_crawl_summary(cfg: &Config, jobs: &[SearchCrawlJob], rejected: &[SearchCrawlRejection]) {
    if !jobs.is_empty() && !cfg.quiet {
        log_info(&format!(
            "search auto-index: queued {} source job(s). Run 'axon serve' if workers are not running.",
            jobs.len()
        ));
    }
    if !rejected.is_empty() && !cfg.json_output {
        log_warn(&format!(
            "search auto-index: {} URL(s) could not be queued; indexing is partial",
            rejected.len()
        ));
        for r in rejected {
            let url = r.url.as_deref().unwrap_or("<missing>");
            log_warn(&format!("  - {url}: {}", r.reason));
        }
    }
}

pub(crate) fn print_search_results(query: &str, results: &[Value]) {
    println!("{}", primary(&format!("Search Results for \"{query}\"")));
    println!("{} {}\n", muted("Found"), results.len());

    for result in results {
        let position = result["position"].as_i64().unwrap_or(0);
        let title = result["title"].as_str().unwrap_or("");
        let url = result["url"].as_str().unwrap_or("");
        let title_prefix_chars = position.to_string().chars().count() + 2;
        println!(
            "{}. {}",
            position,
            primary(&truncate_display_text(
                title,
                120usize.saturating_sub(title_prefix_chars)
            ))
        );
        println!(
            "   {}",
            muted(&truncate_display_continuation(url, SEARCH_ITEM_INDENT))
        );
        if let Some(s) = result["snippet"].as_str() {
            println!("   {}", summarize_snippet(s));
        }
        println!();
    }
}

fn summarize_snippet(snippet: &str) -> String {
    let compact = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= HUMAN_SNIPPET_LIMIT {
        return truncate_display_continuation(&compact, SEARCH_ITEM_INDENT);
    }

    let boundary = compact.floor_char_boundary(HUMAN_SNIPPET_LIMIT);
    let mut truncated = compact[..boundary].trim_end().to_string();
    truncated.push_str("...");
    truncate_display_continuation(&truncated, SEARCH_ITEM_INDENT)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
