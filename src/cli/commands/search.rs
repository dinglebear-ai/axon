use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::context::ServiceContext;
use crate::services::search_crawl::{SearchCrawlJob, SearchCrawlRejection, search_and_crawl};
use crate::services::types::SearchOptions as ServiceSearchOptions;
use serde_json::Value;
use std::error::Error;

pub async fn run_search(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "search requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
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
    let result = search_and_crawl(cfg, service_context, &query, opts).await?;
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
            "search completed, but no result URLs were queued for crawl; first failure: {reason}"
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
            "search auto-index: queued {} crawl job(s). Run 'axon serve' or 'axon crawl worker' if workers are not running.",
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

fn print_search_results(query: &str, results: &[Value]) {
    println!("{}", primary(&format!("Search Results for \"{query}\"")));
    println!("{} {}\n", muted("Found"), results.len());

    for result in results {
        let position = result["position"].as_i64().unwrap_or(0);
        let title = result["title"].as_str().unwrap_or("");
        let url = result["url"].as_str().unwrap_or("");
        println!("{}. {}", position, primary(title));
        println!("   {}", muted(url));
        if let Some(s) = result["snippet"].as_str() {
            println!("   {s}");
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::CommandKind;
    use crate::services::types::ServiceTimeRange;

    fn make_search_cfg(key: &str, query: &str) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Search;
        cfg.positional = vec![query.to_string()];
        cfg.tavily_api_key = key.to_string();
        cfg
    }

    #[test]
    fn search_cfg_time_range_defaults_to_none() {
        let cfg = make_search_cfg("tvly-key", "rust async");
        assert!(
            cfg.search_time_range.is_none(),
            "search_time_range should default to None"
        );
    }

    #[test]
    fn parse_search_time_range_supports_known_values() {
        assert!(matches!(
            parse_service_time_range(Some("day")),
            Some(ServiceTimeRange::Day)
        ));
        assert!(matches!(
            parse_service_time_range(Some("week")),
            Some(ServiceTimeRange::Week)
        ));
        assert!(matches!(
            parse_service_time_range(Some("month")),
            Some(ServiceTimeRange::Month)
        ));
        assert!(matches!(
            parse_service_time_range(Some("year")),
            Some(ServiceTimeRange::Year)
        ));
    }

    #[test]
    fn parse_search_time_range_rejects_unknown_values() {
        assert!(parse_service_time_range(Some("decade")).is_none());
        assert!(parse_service_time_range(Some("")).is_none());
        assert!(parse_service_time_range(None).is_none());
    }

    #[tokio::test]
    async fn run_search_rejects_empty_tavily_key() {
        // run_search bails before touching service_context when the key is empty,
        // so we can use the search_crawl test helpers directly.
        use crate::services::search_crawl::tests::make_noop_ctx;
        let cfg = make_search_cfg("", "rust async");
        let ctx = make_noop_ctx();
        let err = run_search(&cfg, &ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }
}
