use super::map_scrape_payload;
use super::{
    run_with_scrape_batch_timeout, scrape, scrape_batch, scrape_with_vertical_timeout,
    validate_and_normalize_scrape_url,
};
use crate::core::config::Config;
use crate::services::events::{LogLevel, ServiceEvent};
use std::time::Duration;
use tokio::sync::mpsc;

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

#[tokio::test]
async fn scrape_batch_timeout_error_is_deterministic() {
    let err = run_with_scrape_batch_timeout(Duration::from_secs(1), async {
        std::future::pending::<
            Result<Vec<crate::services::types::ScrapeResult>, super::ScrapeBatchError>,
        >()
        .await
    })
    .await
    .expect_err("pending batch should time out");

    assert_eq!(err.to_string(), "scrape batch timed out after 1s");
}

#[tokio::test]
async fn scrape_emits_start_log_event_during_validation() {
    let (tx, mut rx) = mpsc::channel(4);

    let _ = validate_and_normalize_scrape_url("http://127.0.0.1/admin", &Some(tx)).await;

    let event = rx.recv().await.expect("start event");
    assert_eq!(
        event,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "scrape starting: http://127.0.0.1/admin".to_string(),
        }
    );
}

#[tokio::test]
async fn vertical_extractor_timeout_returns_error_instead_of_generic_fallback() {
    let cfg = Config {
        enable_verticals: true,
        ..Config::default()
    };

    let err = scrape_with_vertical_timeout(
        &cfg,
        "https://github.com/rust-lang/rust",
        None,
        Duration::ZERO,
    )
    .await
    .expect_err("vertical timeout should be visible");

    assert!(
        err.to_string().contains("vertical extractor timed out"),
        "unexpected error: {err}"
    );
}

// ── extract_markdown_links ────────────────────────────────────────────────────

#[test]
fn extract_markdown_links_finds_http_and_https() {
    let md = "See [docs](https://docs.rs/foo) and [home](http://example.com).";
    let links = super::extract_markdown_links(md);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0]["href"], "https://docs.rs/foo");
    assert_eq!(links[0]["text"], "docs");
    assert_eq!(links[1]["href"], "http://example.com");
    assert_eq!(links[1]["text"], "home");
}

#[test]
fn extract_markdown_links_ignores_relative_and_anchor_links() {
    let md = "See [page](/relative) and [section](#anchor) and [abs](https://ok.com).";
    let links = super::extract_markdown_links(md);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["href"], "https://ok.com");
}

#[test]
fn extract_markdown_links_empty_markdown_returns_empty() {
    assert!(super::extract_markdown_links("").is_empty());
    assert!(super::extract_markdown_links("No links here at all.").is_empty());
}

#[test]
fn vertical_doc_to_scrape_result_preserves_capped_structured_summary() {
    let result = super::vertical_doc_to_scrape_result(crate::extract::ScrapedDoc {
        url: "https://pypi.org/project/ruff/".to_string(),
        markdown: "# ruff\n\nFast Python linter.".to_string(),
        title: Some("ruff".to_string()),
        extractor_name: "pypi",
        extractor_version: 3,
        structured: Some(serde_json::json!({
            "name": "ruff",
            "api_token": "secret-value-that-must-not-leak"
        })),
        follow_crawl_urls: vec!["https://docs.astral.sh/ruff/".to_string()],
        extra: Some(serde_json::json!({"pkg_name": "ruff"})),
    })
    .expect("scrape result");

    assert_eq!(result.extractor_name.as_deref(), Some("pypi"));
    assert_eq!(result.extra.as_ref().unwrap()["extractor_version"], 3);
    let structured = result.structured.as_ref().expect("structured summary");
    assert_eq!(structured["name"], "ruff");
    assert!(structured.get("api_token").is_none());
    let embedding_structured = result
        .structured_for_embedding
        .as_ref()
        .expect("embedding structured");
    assert_eq!(embedding_structured["name"], "ruff");
    assert!(embedding_structured.get("api_token").is_none());
}

#[test]
fn vertical_structured_summary_drops_oversized_payload() {
    let large = "x".repeat(crate::services::scrape::MAX_PUBLIC_STRUCTURED_BYTES + 1);
    let result = super::vertical_doc_to_scrape_result(crate::extract::ScrapedDoc {
        url: "https://example.com/large".to_string(),
        markdown: "# Large".to_string(),
        title: None,
        extractor_name: "example",
        extractor_version: 1,
        structured: Some(serde_json::json!({"large": large})),
        follow_crawl_urls: Vec::new(),
        extra: None,
    })
    .expect("scrape result");

    assert!(result.structured.is_none());
    assert!(
        result.structured_for_embedding.is_some(),
        "large redacted structured payload should remain available for embedding"
    );
}

#[tokio::test]
async fn scrape_result_embedding_uses_markdown_not_public_output() {
    let mut result = map_scrape_payload(serde_json::json!({
        "url": "https://example.com/package",
        "markdown": "# Package\n\nbody"
    }))
    .expect("scrape result");
    result.output = "<article>Package</article>".to_string();
    result.extractor_name = Some("example".to_string());

    let prepared = super::scrape_result_to_prepared_doc(&Config::default(), &result)
        .await
        .expect("prepared");

    assert_eq!(result.output, "<article>Package</article>");
    assert!(
        prepared
            .chunks
            .iter()
            .any(|chunk| chunk.contains("# Package"))
    );
    assert!(
        !prepared
            .chunks
            .iter()
            .any(|chunk| chunk.contains("<article>"))
    );
}
