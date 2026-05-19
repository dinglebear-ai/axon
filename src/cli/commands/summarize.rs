use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::summarize as summarize_svc;
use crate::services::types::SummarizeResult;
use std::error::Error;

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
    let result = summarize_svc::summarize(cfg, &urls, None).await?;
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
