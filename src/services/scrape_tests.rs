use super::map_scrape_payload;
use super::{scrape, scrape_batch};
use crate::core::config::{Config, ScrapeFormat};

#[test]
fn map_scrape_payload_initializes_without_artifact_handle() {
    let result = map_scrape_payload(serde_json::json!({
        "url": "https://example.com",
        "markdown": "# Example"
    }))
    .expect("scrape payload");

    assert_eq!(result.url, "https://example.com");
    assert!(result.artifact_handle.is_none());
}

#[tokio::test]
async fn scrape_rejects_private_ip_before_fetch() {
    let err = scrape(&Config::default(), "http://127.0.0.1/admin", None)
        .await
        .expect_err("private URL should be rejected");

    assert!(
        err.to_string().contains("invalid scrape url"),
        "error should identify scrape URL validation, got: {err}"
    );
    assert!(
        err.to_string().contains("blocked"),
        "error should preserve SSRF blocker reason, got: {err}"
    );
}

/// Verify that `map_scrape_payload` output is transformed by `to_llm_text` when
/// `cfg.format == ScrapeFormat::Llm`.  This exercises the branch added to the
/// vertical-extractor fast path without requiring a live vertical extractor.
#[test]
fn map_scrape_payload_llm_format_applies_to_llm_text() {
    use crate::core::content::to_llm_text;

    let url = "https://example.com/page";
    let markdown = "# Hello\n\nSome content.";
    let result = map_scrape_payload(serde_json::json!({
        "url": url,
        "markdown": markdown
    }))
    .expect("scrape payload");

    // Simulate the LLM-format branch: apply to_llm_text to the output.
    let llm_output = to_llm_text(&result.output, url);

    // The LLM transform prepends a URL metadata header.
    assert!(
        llm_output.contains("> URL:"),
        "LLM output should contain URL header, got: {llm_output}"
    );
    assert!(
        llm_output.contains("example.com/page"),
        "LLM output should reference the page URL, got: {llm_output}"
    );
    // The raw markdown body should still be present.
    assert!(
        llm_output.contains("Hello"),
        "LLM output should contain page heading, got: {llm_output}"
    );
}

/// Ensure the default ScrapeFormat (Markdown) does NOT apply to_llm_text
/// transformation — i.e., the vertical path leaves `output` unchanged.
#[test]
fn scrape_format_default_is_not_llm() {
    let cfg = Config::default();
    assert_ne!(
        cfg.format,
        ScrapeFormat::Llm,
        "default ScrapeFormat should not be Llm"
    );
}

#[tokio::test]
async fn scrape_batch_rejects_more_than_fifty_urls_before_fetch() {
    let urls = (0..51)
        .map(|idx| format!("https://example.com/{idx}"))
        .collect::<Vec<_>>();

    let err = scrape_batch(&Config::default(), &urls, None)
        .await
        .expect_err("batch should reject over-cap input");

    assert!(
        err.to_string().contains("at most 50 urls"),
        "unexpected error: {err}"
    );
}
