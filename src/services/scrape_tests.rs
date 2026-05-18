use super::map_scrape_payload;
use super::{scrape, scrape_batch};
use crate::core::config::Config;

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
