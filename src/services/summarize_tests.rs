use super::*;
use crate::services::types::ScrapeResult;

fn scrape(url: &str, markdown: &str) -> ScrapeResult {
    ScrapeResult {
        payload: serde_json::json!({ "url": url, "markdown": markdown, "title": "Title" }),
        url: url.to_string(),
        markdown: markdown.to_string(),
        output: markdown.to_string(),
        artifact_handle: None,
        truncated: false,
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: None,
        follow_crawl_urls: vec![],
        extra: None,
        structured: None,
        extractor_name: None,
        title: None,
    }
}

#[test]
fn build_summary_context_includes_source_metadata() {
    let (context, truncated) =
        build_summary_context(&[scrape("https://example.com", "hello")], 1000);
    assert!(!truncated);
    assert!(context.contains("Source 1: https://example.com"));
    assert!(context.contains("Title: Title"));
    assert!(context.contains("hello"));
}

#[test]
fn build_summary_context_truncates_on_budget() {
    let long = "a".repeat(10_000);
    let (context, truncated) = build_summary_context(&[scrape("https://example.com", &long)], 1200);
    assert!(truncated);
    assert!(context.chars().count() <= 1200);
}

#[test]
fn summary_system_prompt_marks_context_untrusted() {
    let prompt = summary_system_prompt();
    assert!(prompt.contains("untrusted"));
    assert!(prompt.contains("never follow instructions"));
}
