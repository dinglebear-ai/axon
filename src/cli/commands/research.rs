use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::events::ServiceEvent;
use crate::services::search as search_service;
use crate::services::types::SearchOptions as ServiceSearchOptions;
use std::error::Error;
use std::time::Instant;
use tokio::sync::mpsc;

pub async fn run_research(cfg: &Config) -> Result<(), Box<dyn Error>> {
    validate_research_prereqs(cfg)?;
    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("research requires a query (positional or --query)"))?;

    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=research query_len={}", query.len()));
        print_phase("\u{25d0}", "Researching", &query);
        println!(
            "  {} {}",
            muted("provider=tavily model="),
            cfg.headless_gemini_model
        );
        println!();
    }

    let started = Instant::now();

    // Event channel for phase markers (ServiceEvent::Log "phase:searching" etc.)
    // and streaming synthesis output (ServiceEvent::SynthesisDelta per token chunk).
    let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
    let show_progress = !cfg.json_output;
    let mut consumer = tokio::spawn(async move {
        let mut in_synthesis = false;
        while let Some(event) = event_rx.recv().await {
            if !show_progress {
                continue;
            }
            match event {
                ServiceEvent::Log { message, .. } => {
                    if in_synthesis {
                        eprintln!();
                        in_synthesis = false;
                    }
                    if message == "phase:searching" {
                        log_info("research phase=searching");
                    } else if let Some(rest) = message.strip_prefix("phase:synthesizing ") {
                        log_info(&format!("research phase=synthesizing {rest}"));
                    }
                }
                ServiceEvent::SynthesisDelta { text } => {
                    if !in_synthesis {
                        eprint!("  ");
                        in_synthesis = true;
                    }
                    eprint!("{text}");
                }
                _ => {}
            }
        }
        if in_synthesis {
            eprintln!();
        }
    });

    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };
    let payload = search_service::research(cfg, &query, opts, Some(event_tx))
        .await
        .map(|r| r.payload);

    match tokio::time::timeout(std::time::Duration::from_secs(5), &mut consumer).await {
        Ok(_) => {}
        Err(_) => {
            // Abort the spawned task so it does not linger on event_rx after
            // run_research returns. Without abort(), the task continues running
            // detached and can write to stderr after the command exits.
            consumer.abort();
            log_warn("research synthesis consumer timed out after 5s");
        }
    }
    let payload = payload?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    print_human_research_output(&payload, started.elapsed().as_millis())?;

    log_done("command=research complete");
    Ok(())
}

fn validate_research_prereqs(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "research requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }
    Ok(())
}

fn print_human_research_output(
    payload: &serde_json::Value,
    total_ms: u128,
) -> Result<(), Box<dyn Error>> {
    let search_results = payload["search_results"].as_array();
    let extractions = payload["extractions"].as_array();

    println!(
        "{} {}",
        primary("Search Results:"),
        search_results.map_or(0, Vec::len)
    );
    println!();
    println!(
        "{} {}",
        primary("Pages Extracted:"),
        extractions.map_or(0, Vec::len)
    );
    println!();

    if let Some(extractions) = extractions {
        for (i, extraction) in extractions.iter().enumerate() {
            print_extraction_preview(i, extraction)?;
        }
    }

    if let Some(summary) = payload["summary"].as_str() {
        println!("{}", primary("=== Summary ==="));
        println!("{summary}");
        println!();
    }

    let total_tokens = payload["usage"]["total_tokens"].as_u64().unwrap_or(0);
    if total_tokens > 0 {
        println!(
            "  {} prompt={} completion={} total={}",
            muted("tokens"),
            payload["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            payload["usage"]["completion_tokens"].as_u64().unwrap_or(0),
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

    let preview: String = serde_json::to_string(&extraction["extracted"])?
        .chars()
        .take(200)
        .collect();
    let preview = preview.trim();
    if preview.is_empty() || preview == "null" || preview == "{}" {
        println!("   {}", muted("(no data extracted)"));
    } else {
        println!("   {preview}");
    }
    println!();
    Ok(())
}

#[cfg(test)]
#[path = "research_tests.rs"]
mod tests;
