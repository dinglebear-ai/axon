use crate::crates::cli::commands::common::parse_service_time_range;
use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::core::ui::{muted, primary, print_phase};
use crate::crates::services::search::search_batch;
use crate::crates::services::types::SearchOptions as ServiceSearchOptions;
use std::error::Error;

pub async fn run_search(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "search requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }

    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("search requires a query (positional or --query)"))?;

    // TODO: cfg.quiet — suppress progress logs when quiet mode lands
    if !cfg.json_output {
        log_info(&format!("command=search query_len={}", query.len()));
        print_phase("\u{25d0}", "Searching", &query);
    }

    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };

    let search_start = std::time::Instant::now();
    let results = search_batch(cfg, &[query.as_str()], opts, None)
        .await?
        .results;
    let duration_ms = search_start.elapsed().as_millis();

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "query": query,
                "limit": cfg.search_limit,
                "offset": 0,
                "search_time_range": cfg.search_time_range.as_deref(),
                "results": results,
            }))?
        );
        return Ok(());
    }

    println!("{}", primary(&format!("Search Results for \"{query}\"")));
    println!("{} {}\n", muted("Found"), results.len());

    for result in &results {
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

    if !cfg.json_output {
        log_done(&format!(
            "command=search complete query_len={} results={} duration_ms={duration_ms}",
            query.len(),
            results.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::CommandKind;
    use crate::crates::core::logging::log_warn;
    use spider_agent::TimeRange;

    fn make_search_cfg(key: &str, query: &str) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Search;
        cfg.positional = vec![query.to_string()];
        cfg.tavily_api_key = key.to_string();
        cfg
    }

    fn parse_search_time_range(value: Option<&str>) -> Option<TimeRange> {
        match value.map(str::trim).filter(|v| !v.is_empty()) {
            Some("day") => Some(TimeRange::Day),
            Some("week") => Some(TimeRange::Week),
            Some("month") => Some(TimeRange::Month),
            Some("year") => Some(TimeRange::Year),
            Some(other) => {
                log_warn(&format!("Unknown search_time_range '{other}'; ignoring"));
                None
            }
            None => None,
        }
    }

    #[tokio::test]
    async fn test_run_search_rejects_empty_tavily_key() {
        let cfg = make_search_cfg("", "rust async");
        let err = run_search(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
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
            parse_search_time_range(Some("day")),
            Some(TimeRange::Day)
        ));
        assert!(matches!(
            parse_search_time_range(Some("week")),
            Some(TimeRange::Week)
        ));
        assert!(matches!(
            parse_search_time_range(Some("month")),
            Some(TimeRange::Month)
        ));
        assert!(matches!(
            parse_search_time_range(Some("year")),
            Some(TimeRange::Year)
        ));
    }

    #[test]
    fn parse_search_time_range_rejects_unknown_values() {
        assert!(parse_search_time_range(Some("decade")).is_none());
        assert!(parse_search_time_range(Some("")).is_none());
        assert!(parse_search_time_range(None).is_none());
    }
}
