use crate::crates::core::logging::log_warn;
use crate::crates::core::ui::{error, muted};
use crate::crates::jobs::crawl::CrawlJob;
use crate::crates::jobs::embed::EmbedJob;
use crate::crates::jobs::extract::ExtractJob;
use crate::crates::jobs::graph::GraphJob;
use crate::crates::jobs::ingest::IngestJob;
use crate::crates::jobs::refresh::RefreshJob;

#[allow(dead_code)]
fn classify_error(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("connection refused")
        || lower.contains("connect error")
        || lower.contains("network")
        || lower.contains("dns")
        || lower.contains("unreachable")
    {
        "network"
    } else if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
    {
        "rate-limited"
    } else if lower.contains("404") || lower.contains("not found") {
        "not-found"
    } else {
        "other"
    }
}

#[allow(dead_code)]
pub fn print_failure_summary(
    crawl_jobs: &[CrawlJob],
    extract_jobs: &[ExtractJob],
    embed_jobs: &[EmbedJob],
    ingest_jobs: &[IngestJob],
    refresh_jobs: &[RefreshJob],
    graph_jobs: &[GraphJob],
) {
    let error_texts: Vec<&str> = crawl_jobs
        .iter()
        .filter(|j| j.status == "failed")
        .filter_map(|j| j.error_text.as_deref())
        .chain(
            extract_jobs
                .iter()
                .filter(|j| j.status == "failed")
                .filter_map(|j| j.error_text.as_deref()),
        )
        .chain(
            embed_jobs
                .iter()
                .filter(|j| j.status == "failed")
                .filter_map(|j| j.error_text.as_deref()),
        )
        .chain(
            ingest_jobs
                .iter()
                .filter(|j| j.status == "failed")
                .filter_map(|j| j.error_text.as_deref()),
        )
        .chain(
            refresh_jobs
                .iter()
                .filter(|j| j.status == "failed")
                .filter_map(|j| j.error_text.as_deref()),
        )
        .chain(
            graph_jobs
                .iter()
                .filter(|j| j.status == "failed")
                .filter_map(|j| j.error_text.as_deref()),
        )
        .collect();

    if error_texts.is_empty() {
        return;
    }

    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for text in &error_texts {
        *counts.entry(classify_error(text)).or_insert(0) += 1;
    }

    let mut parts: Vec<String> = Vec::new();
    for cat in &["timeout", "network", "rate-limited", "not-found"] {
        if let Some(&count) = counts.get(cat) {
            let label = match *cat {
                "timeout" => "timeouts",
                "network" => "network errors",
                "rate-limited" => "rate-limited",
                "not-found" => "not found",
                _ => cat,
            };
            parts.push(format!("{count} {label}"));
        }
    }
    let other_count = counts.get("other").copied().unwrap_or(0);
    if other_count > 0 {
        parts.push(format!("{other_count} other"));
    }

    if parts.is_empty() {
        log_warn(&format!(
            "  {} {}",
            error(&format!("{} failures", error_texts.len())),
            muted("(see individual jobs below)")
        ));
    } else {
        log_warn(&format!("  {} {}", error("Failures:"), parts.join(", ")));
    }
}
