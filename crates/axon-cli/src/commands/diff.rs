use axon_core::config::Config;
use axon_core::logging::{log_done, log_info};
use axon_core::ui::{muted, primary, print_option, print_phase};
use axon_services::diff as diff_svc;
use axon_services::types::{DiffResult, DiffStatus};
use std::error::Error;

pub async fn run_diff(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let (url_a, url_b) = parse_diff_urls(cfg)?;

    log_info(&format!("command=diff url_a={url_a} url_b={url_b}"));
    let result = diff_svc::diff(cfg, &url_a, &url_b, None).await?;

    emit_diff_result(cfg, &result)?;

    log_done(&format!(
        "command=diff status={:?} metadata_changes={} links_added={} links_removed={}",
        result.status,
        result.metadata_changes.len(),
        result.links_added.len(),
        result.links_removed.len(),
    ));
    Ok(())
}

fn parse_diff_urls(cfg: &Config) -> Result<(String, String), Box<dyn Error>> {
    match cfg.positional.as_slice() {
        [a, b] => Ok((a.clone(), b.clone())),
        [_, _, _, ..] => Err(format!(
            "diff takes exactly two URLs but {} were given: axon diff <url-a> <url-b>",
            cfg.positional.len()
        )
        .into()),
        [_] => Err("diff requires two URLs: axon diff <url-a> <url-b>".into()),
        [] => Err("diff requires two URLs: axon diff <url-a> <url-b>".into()),
    }
}

pub(crate) fn emit_diff_result(cfg: &Config, result: &DiffResult) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    let status_label = match result.status {
        DiffStatus::Same => "no changes",
        DiffStatus::Changed => "changed",
    };
    print_phase(
        "◑",
        "Diff",
        &format!("{} vs {}", result.url_a, result.url_b),
    );
    print_option("status", status_label);
    print_option("wordCountDelta", &format!("{:+}", result.word_count_delta));
    print_option(
        "metadataChanges",
        &result.metadata_changes.len().to_string(),
    );
    print_option("linksAdded", &result.links_added.len().to_string());
    print_option("linksRemoved", &result.links_removed.len().to_string());

    if !result.metadata_changes.is_empty() {
        println!("\n{}", primary("Metadata Changes"));
        for change in &result.metadata_changes {
            let old = change.old.as_deref().unwrap_or("(none)");
            let new = change.new.as_deref().unwrap_or("(none)");
            println!("  {} {}: {} → {}", muted("~"), change.field, old, new);
        }
    }

    if let Some(ref diff_text) = result.text_diff {
        println!("\n{}", primary("Content Diff"));
        println!("{diff_text}");
    }

    if !result.links_added.is_empty() {
        println!("\n{}", primary("Links Added"));
        for link in &result.links_added {
            println!("  {} {} ({})", muted("+"), link.href, link.text);
        }
    }

    if !result.links_removed.is_empty() {
        println!("\n{}", primary("Links Removed"));
        for link in &result.links_removed {
            println!("  {} {} ({})", muted("-"), link.href, link.text);
        }
    }

    Ok(())
}

/// Pure formatting helper exposed for testing.
#[cfg(test)]
pub(crate) fn format_diff_summary(result: &DiffResult) -> String {
    match result.status {
        DiffStatus::Same => format!(
            "same (no changes) word_count_delta={:+}",
            result.word_count_delta
        ),
        DiffStatus::Changed => format!(
            "changed word_count_delta={:+} metadata={} links_added={} links_removed={}",
            result.word_count_delta,
            result.metadata_changes.len(),
            result.links_added.len(),
            result.links_removed.len(),
        ),
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
