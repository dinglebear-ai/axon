use crate::crates::cli::commands::common::parse_service_time_range;
use crate::crates::core::config::Config;
#[cfg(test)]
use crate::crates::core::logging::log_warn;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::core::ui::{muted, primary, print_phase};
use crate::crates::services::search as search_service;
use crate::crates::services::types::SearchOptions as ServiceSearchOptions;
#[cfg(test)]
use spider_agent::TimeRange;
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

pub async fn run_research(cfg: &Config) -> Result<(), Box<dyn Error>> {
    validate_research_prereqs(cfg)?;
    let query = resolve_research_query(cfg)?;

    log_info(&format!("command=research query_len={}", query.len()));
    if !cfg.json_output {
        print_phase("◐", "Researching", &query);
        println!("  {} {}", muted("provider=tavily model="), cfg.openai_model);
        println!();
    }

    let started = Instant::now();
    let running = Arc::new(AtomicBool::new(true));
    let running_tick = Arc::clone(&running);
    let tick_started = started;
    let ticker = if !cfg.json_output {
        Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if !running_tick.load(Ordering::Relaxed) {
                    break;
                }
                log_info(&format!(
                    "research status=in_progress elapsed_ms={}",
                    tick_started.elapsed().as_millis()
                ));
            }
        }))
    } else {
        None
    };

    // Route data-fetch through the services layer.
    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };
    let payload = search_service::research(cfg, &query, opts, None)
        .await
        .map(|r| r.payload);
    running.store(false, Ordering::Relaxed);
    if let Some(t) = ticker {
        let _ = t.await;
    }
    let payload = payload?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        log_done("command=research complete");
        return Ok(());
    }
    print_human_research_output(&payload, started.elapsed().as_millis())?;

    log_done("command=research complete");
    Ok(())
}

fn validate_research_prereqs(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!("research requires TAVILY_API_KEY — set it in .env").into());
    }
    if cfg.openai_base_url.is_empty() || cfg.openai_model.is_empty() {
        return Err(anyhow::anyhow!(
            "research requires OPENAI_BASE_URL and OPENAI_MODEL — set them in .env"
        )
        .into());
    }
    Ok(())
}

fn resolve_research_query(cfg: &Config) -> Result<String, Box<dyn Error>> {
    if let Some(q) = &cfg.query {
        return Ok(q.clone());
    }
    if !cfg.positional.is_empty() {
        return Ok(cfg.positional.join(" "));
    }
    Err(anyhow::anyhow!("research requires a query (positional or --query)").into())
}

fn print_human_research_output(
    payload: &serde_json::Value,
    total_ms: u128,
) -> Result<(), Box<dyn Error>> {
    let search_results = payload["search_results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let extractions = payload["extractions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let summary = payload["summary"].as_str();
    let prompt_tokens = payload["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let completion_tokens = payload["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    let total_tokens = payload["usage"]["total_tokens"].as_u64().unwrap_or(0);

    println!("{} {}", primary("Search Results:"), search_results.len());
    println!();
    println!("{} {}", primary("Pages Extracted:"), extractions.len());
    println!();

    for (i, extraction) in extractions.iter().enumerate() {
        print_extraction_preview(i, extraction)?;
    }

    if let Some(summary) = summary {
        println!("{}", primary("=== Summary ==="));
        println!("{summary}");
        println!();
    }
    if total_tokens > 0 {
        println!(
            "  {} prompt={} completion={} total={}",
            muted("tokens"),
            prompt_tokens,
            completion_tokens,
            total_tokens
        );
    }
    println!("  {} total={}ms", muted("timing"), total_ms);
    Ok(())
}

fn print_extraction_preview(
    i: usize,
    extraction: &serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    let title = extraction["title"].as_str().unwrap_or("");
    let url = extraction["url"].as_str().unwrap_or("");
    println!("{}. {}", i + 1, primary(title));
    println!("   {}", muted(url));
    let preview = serde_json::to_string(&extraction["extracted"])?
        .chars()
        .take(200)
        .collect::<String>();
    let preview = preview.trim();
    if preview.is_empty() || preview == "null" || preview == "{}" {
        println!("   {}", muted("(no data extracted)"));
    } else {
        println!("   {preview}");
    }
    println!();
    Ok(())
}

// Only used in tests via `use super::*` in the test module.
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::CommandKind;

    fn make_research_cfg(tavily_key: &str, openai_url: &str, openai_model: &str) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Research;
        cfg.positional = vec!["test query".to_string()];
        cfg.tavily_api_key = tavily_key.to_string();
        cfg.openai_base_url = openai_url.to_string();
        cfg.openai_api_key = "test-key".to_string();
        cfg.openai_model = openai_model.to_string();
        cfg
    }

    #[tokio::test]
    async fn test_run_research_rejects_empty_tavily_key() {
        let cfg = make_research_cfg("", "http://localhost/v1", "gpt-4o-mini");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_empty_openai_config() {
        let cfg = make_research_cfg("tvly-key", "", "gpt-4o-mini");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("OPENAI_BASE_URL"),
            "expected OPENAI_BASE_URL error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_empty_openai_model() {
        let cfg = make_research_cfg("tvly-key", "http://localhost/v1", "");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("OPENAI_MODEL"),
            "expected OPENAI_MODEL error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_missing_query() {
        let mut cfg = make_research_cfg("tvly-key", "http://localhost/v1", "gpt-4o-mini");
        cfg.positional = vec![];
        cfg.query = None;
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("query"),
            "expected query error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_double_chat_completions() {
        let cfg = make_research_cfg(
            "tvly-key",
            "http://localhost/v1/chat/completions",
            "gpt-4o-mini",
        );
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("should not include /chat/completions"),
            "expected /chat/completions validation error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_invalid_url() {
        let cfg = make_research_cfg("tvly-key", "not a valid url", "gpt-4o-mini");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("invalid OPENAI_BASE_URL"),
            "expected URL parse error, got: {err}"
        );
    }

    #[test]
    fn research_cfg_depth_defaults_to_none() {
        let cfg = make_research_cfg("tvly-key", "http://localhost/v1", "gpt-4o-mini");
        assert!(
            cfg.research_depth.is_none(),
            "research_depth should default to None"
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
