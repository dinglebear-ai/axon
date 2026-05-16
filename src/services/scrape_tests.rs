use super::map_scrape_payload;

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
