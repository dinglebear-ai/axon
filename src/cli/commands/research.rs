use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::context::ServiceContext;
use crate::services::events::ServiceEvent;
use crate::services::search as search_service;
use crate::services::types::{
    ResearchExtraction, ResearchPayload, SearchOptions as ServiceSearchOptions, SummarySource,
};
use std::error::Error;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Channel buffer for streaming synthesis tokens to the CLI consumer task.
/// Sized to comfortably outlast typical Gemini delta bursts (low hundreds
/// of tokens per second) without blocking the emitter.
const RESEARCH_EVENT_CHANNEL: usize = 256;

/// Maximum time we wait for the streaming-progress consumer task to drain
/// after the underlying research call has resolved. If exceeded, we log a
/// warning and detach — see [`run_research`] for the rationale.
const RESEARCH_CONSUMER_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Max characters of the extraction snippet shown in the human-readable
/// preview. UTF-8 char-safe truncation.
const RESEARCH_PREVIEW_CHARS: usize = 200;

/// Entry point for `axon research <query>`.
///
/// Validates the query is non-empty, sets up an mpsc channel to stream
/// synthesis-delta tokens and phase markers to stderr (for the human
/// renderer; suppressed under `--json`), and delegates to the service
/// layer. The service layer is the canonical place where Tavily/Gemini
/// prereqs are enforced, so this handler does not duplicate that check.
///
/// `--research-depth` (when set) overrides `--limit` as the number of
/// sources to incorporate into synthesis. The flag has no effect when
/// unset; callers default to `cfg.search_limit`.
pub async fn run_research(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("research requires a query (positional or --query)"))?;

    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=research query_len={}", query.len()));
        print_phase("\u{25d0}", "Researching", &query);
        let model = crate::core::llm::configured_model_from_config(cfg)
            .unwrap_or_else(|| "(backend default)".to_string());
        let provider = if cfg.searxng_url.is_empty() {
            "tavily"
        } else {
            "searxng"
        };
        println!(
            "  {}{provider} {}{model}",
            muted("provider="),
            muted("model=")
        );
        println!();
    }

    let started = Instant::now();

    // Event channel for phase markers (ServiceEvent::Log "phase:searching" etc.)
    // and streaming synthesis output (ServiceEvent::SynthesisDelta per token chunk).
    let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(RESEARCH_EVENT_CHANNEL);
    let show_progress = !cfg.json_output;
    let mut consumer = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        let mut in_synthesis = false;
        while let Some(event) = event_rx.recv().await {
            if !show_progress {
                continue;
            }
            match event {
                ServiceEvent::Log { message, .. } => {
                    if in_synthesis {
                        let _ = writeln!(stdout);
                        let _ = stdout.flush();
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
                        in_synthesis = true;
                    }
                    let _ = stdout.write_all(text.as_bytes());
                    let _ = stdout.flush();
                }
                _ => {}
            }
        }
        if in_synthesis {
            let _ = writeln!(stdout);
            let _ = stdout.flush();
        }
    });

    // `--research-depth` reinterprets `--limit` for the research command:
    // it is the number of sources the LLM synthesizes over. Falls back to
    // the shared search limit when unset.
    let limit = cfg.research_depth.unwrap_or(cfg.search_limit);
    let opts = ServiceSearchOptions {
        limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };
    let result =
        search_service::research_with_context(cfg, service_context, &query, opts, Some(event_tx))
            .await;

    // Drain the consumer with a bounded timeout. The mpsc sender held by
    // the service call is dropped when the call returns, so the consumer
    // naturally exits on the next `recv()` returning None. If something
    // pathological is holding the consumer (e.g. a slow stderr writer),
    // we warn and abort so it cannot keep writing after the command exits.
    if tokio::time::timeout(RESEARCH_CONSUMER_DRAIN_TIMEOUT, &mut consumer)
        .await
        .is_err()
    {
        consumer.abort();
        log_warn(&format!(
            "research synthesis consumer timed out after {}s draining stderr",
            RESEARCH_CONSUMER_DRAIN_TIMEOUT.as_secs()
        ));
    }

    let payload = result?.payload;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    print_human_research_output(&payload, started.elapsed().as_millis())?;

    log_done("command=research complete");
    Ok(())
}

/// Render the human-readable research summary to stdout.
pub(crate) fn print_human_research_output(
    payload: &ResearchPayload,
    total_ms: u128,
) -> Result<(), Box<dyn Error>> {
    println!(
        "{} {}",
        primary("Search Results:"),
        payload.search_results.len()
    );
    println!();
    println!(
        "{} {}",
        primary("Pages Extracted:"),
        payload.extractions.len()
    );
    println!();

    for (i, extraction) in payload.extractions.iter().enumerate() {
        print_extraction_preview(i, extraction);
    }

    if let Some(summary) = payload.summary.as_deref() {
        match payload.summary_source {
            SummarySource::Fallback => println!(
                "{} {}",
                primary("=== Summary ==="),
                muted("(fallback — LLM synthesis unavailable)")
            ),
            _ => println!("{}", primary("=== Summary ===")),
        }
        println!("{summary}");
        println!();
    }

    if payload.usage.total_tokens > 0 {
        println!(
            "  {} prompt={} completion={} total={}",
            muted("tokens"),
            payload.usage.prompt_tokens,
            payload.usage.completion_tokens,
            payload.usage.total_tokens
        );
    }
    println!("  {} total={total_ms}ms", muted("timing"));
    Ok(())
}

/// Print one extraction's preview (index, title, URL, truncated snippet).
fn print_extraction_preview(i: usize, extraction: &ResearchExtraction) {
    let title = if extraction.title.is_empty() {
        "(untitled)"
    } else {
        extraction.title.as_str()
    };
    println!("{}. {}", i + 1, primary(title));
    println!("   {}", muted(&extraction.url));

    let preview: String = extraction
        .extracted
        .chars()
        .take(RESEARCH_PREVIEW_CHARS)
        .collect();
    let trimmed = preview.trim();
    if trimmed.is_empty() {
        println!("   {}", muted("(no data extracted)"));
    } else {
        println!("   {trimmed}");
    }
    println!();
}

#[cfg(test)]
#[path = "research_tests.rs"]
mod tests;
