use crate::crates::cli::commands::common::parse_service_time_range;
use crate::crates::core::config::Config;
#[cfg(test)]
use crate::crates::core::logging::log_warn;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::core::ui::{muted, primary, print_phase};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::search as search_service;
use crate::crates::services::types::SearchOptions as ServiceSearchOptions;
#[cfg(test)]
use spider_agent::TimeRange;
use std::error::Error;
use std::time::Instant;
use tokio::sync::mpsc;

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

    // Wire up event channel for phase markers and streaming synthesis output.
    // The service layer emits:
    //   - ServiceEvent::Log { message: "phase:searching" | "phase:synthesizing results=N" }
    //   - ServiceEvent::SynthesisDelta { text } — one event per LLM token chunk
    let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
    let show_progress = !cfg.json_output;
    let consumer = tokio::spawn(async move {
        let mut in_synthesis = false;
        while let Some(event) = event_rx.recv().await {
            if !show_progress {
                continue;
            }
            match event {
                ServiceEvent::Log { message, .. } => {
                    if in_synthesis {
                        eprintln!(); // end streaming line before the next log
                        in_synthesis = false;
                    }
                    if message == "phase:searching" {
                        log_info("research phase=searching");
                    } else if let Some(rest) = message.strip_prefix("phase:synthesizing ") {
                        log_info(&format!("research phase=synthesizing {rest}"));
                    }
                    // other log messages (starting/complete) are ignored — run_research
                    // already prints its own header
                }
                ServiceEvent::SynthesisDelta { text } => {
                    if !in_synthesis {
                        eprint!("  "); // indent the inline stream
                        in_synthesis = true;
                    }
                    eprint!("{text}");
                }
                _ => {}
            }
        }
        if in_synthesis {
            eprintln!(); // flush streaming line if channel closed mid-synthesis
        }
    });

    // Route data-fetch through the services layer.
    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };
    let payload = search_service::research(cfg, &query, opts, Some(event_tx))
        .await
        .map(|r| r.payload);
    // Await the consumer with a short timeout — it exits once the sender is dropped
    // (research() returned), but guard against a hung consumer blocking the command.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), consumer).await;
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
    if cfg
        .acp_adapter_cmd
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(
            anyhow::anyhow!("research requires AXON_ACP_ADAPTER_CMD — set it in .env").into(),
        );
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

    fn make_research_cfg(
        tavily_key: &str,
        adapter_cmd: Option<&str>,
        openai_model: &str,
    ) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Research;
        cfg.positional = vec!["test query".to_string()];
        cfg.tavily_api_key = tavily_key.to_string();
        cfg.acp_adapter_cmd = adapter_cmd.map(ToString::to_string);
        cfg.openai_model = openai_model.to_string();
        cfg
    }

    #[tokio::test]
    async fn test_run_research_rejects_empty_tavily_key() {
        let cfg = make_research_cfg("", Some("codex"), "gpt-4o-mini");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_empty_acp_adapter() {
        let cfg = make_research_cfg("tvly-key", None, "gpt-4o-mini");
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("AXON_ACP_ADAPTER_CMD"),
            "expected AXON_ACP_ADAPTER_CMD error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_research_rejects_missing_query() {
        let mut cfg = make_research_cfg("tvly-key", Some("codex"), "gpt-4o-mini");
        cfg.positional = vec![];
        cfg.query = None;
        let err = run_research(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("query"),
            "expected query error, got: {err}"
        );
    }

    #[test]
    fn research_cfg_depth_defaults_to_none() {
        let cfg = make_research_cfg("tvly-key", Some("codex"), "gpt-4o-mini");
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
