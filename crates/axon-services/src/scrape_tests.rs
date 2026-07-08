use super::map_scrape_payload;
use super::{
    run_with_scrape_batch_timeout, scrape, scrape_batch, validate_and_normalize_scrape_url,
};
use crate::events::{LogLevel, ServiceEvent};
use axon_core::config::Config;
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
        std::future::pending::<Result<Vec<crate::types::ScrapeResult>, super::ScrapeBatchError>>()
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
            .chunks()
            .iter()
            .any(|chunk| chunk.contains("# Package"))
    );
    assert!(
        !prepared
            .chunks()
            .iter()
            .any(|chunk| chunk.contains("<article>"))
    );
}
