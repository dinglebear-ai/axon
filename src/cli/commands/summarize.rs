use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::events::ServiceEvent;
use crate::services::summarize as summarize_svc;
use crate::services::types::SummarizeResult;
use std::error::Error;
use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc;

/// Timeout for draining the streaming consumer after summarize returns.
const SUMMARIZE_CONSUMER_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run_summarize(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err("summarize requires at least one URL (positional or --urls)".into());
    }

    log_info(&format!(
        "command=summarize urls={} render_mode={:?}",
        urls.len(),
        cfg.render_mode
    ));

    let (event_tx, event_rx) = if !cfg.json_output {
        let (tx, rx) = mpsc::channel::<ServiceEvent>(256);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let mut consumer = event_rx.map(|mut rx| {
        tokio::spawn(async move {
            let mut stdout = std::io::stdout();
            let mut started = false;
            while let Some(event) = rx.recv().await {
                if let ServiceEvent::SynthesisDelta { text } = event {
                    if !started {
                        started = true;
                    }
                    let _ = stdout.write_all(text.as_bytes());
                    let _ = stdout.flush();
                }
            }
            if started {
                let _ = writeln!(stdout);
            }
        })
    });

    let result = summarize_svc::summarize(cfg, &urls, event_tx).await?;

    if let Some(ref mut task) = consumer
        && tokio::time::timeout(SUMMARIZE_CONSUMER_DRAIN_TIMEOUT, &mut *task)
            .await
            .is_err()
    {
        task.abort();
        log_warn("summarize synthesis consumer timed out");
    }

    emit_summarize_result(cfg, &result)?;
    log_done(&format!(
        "command=summarize urls={} context_chars={} truncated={}",
        result.urls.len(),
        result.context_chars,
        result.context_truncated
    ));
    Ok(())
}

pub(crate) fn emit_summarize_result(
    cfg: &Config,
    result: &SummarizeResult,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    let target = match result.urls.as_slice() {
        [single] => single.as_str(),
        [first, rest @ ..] => return emit_multi_summary(first, rest.len(), result),
        [] => "summarize",
    };
    print_phase("◐", "Summarized", target);
    print_option("sources", &result.documents.len().to_string());
    print_option("contextChars", &result.context_chars.to_string());
    print_option("contextTruncated", &result.context_truncated.to_string());
    println!();
    println!("{}", primary("Summary"));
    println!("{}", result.summary);
    Ok(())
}

fn emit_multi_summary(
    first: &str,
    remaining: usize,
    result: &SummarizeResult,
) -> Result<(), Box<dyn Error>> {
    print_phase("◐", "Summarized", &format!("{first} (+{remaining} more)"));
    print_option("sources", &result.documents.len().to_string());
    print_option("contextChars", &result.context_chars.to_string());
    print_option("contextTruncated", &result.context_truncated.to_string());
    println!("  {} {}", muted("URLs:"), result.urls.join(", "));
    println!();
    println!("{}", primary("Summary"));
    println!("{}", result.summary);
    Ok(())
}
