use super::*;

#[test]
fn parses_searxng_json_results() {
    let body = r#"{
        "query": "q",
        "number_of_results": 0,
        "results": [
            {"url": "https://a.example/x", "title": "A", "content": "snippet a", "engine": "startpage"},
            {"url": "https://b.example/y", "title": "B", "content": "snippet b"}
        ]
    }"#;
    let parsed: SearxResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.results.len(), 2);
    assert_eq!(parsed.results[0].url, "https://a.example/x");
    assert_eq!(parsed.results[0].title, "A");
    assert_eq!(parsed.results[0].content, "snippet a");
}

#[test]
fn missing_optional_fields_default_to_empty() {
    let body = r#"{"results": [{"url": "https://a.example/x"}]}"#;
    let parsed: SearxResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.results.len(), 1);
    assert_eq!(parsed.results[0].title, "");
    assert_eq!(parsed.results[0].content, "");
}

#[test]
fn absent_results_array_is_empty_not_error() {
    let parsed: SearxResponse = serde_json::from_str(r#"{"query":"q"}"#).unwrap();
    assert!(parsed.results.is_empty());
}

#[test]
fn time_range_maps_to_searxng_values() {
    assert_eq!(time_range_param(TimeRange::Day), Some("day"));
    assert_eq!(time_range_param(TimeRange::Week), Some("week"));
    assert_eq!(time_range_param(TimeRange::Month), Some("month"));
    assert_eq!(time_range_param(TimeRange::Year), Some("year"));
}

#[tokio::test]
async fn searxng_search_parses_mock_results() {
    use crate::core::config::Config;
    use httpmock::MockServer;
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "results": [
                        {"url": "https://a.example/x", "title": "A", "content": "snippet a"},
                        {"url": "https://b.example/y", "title": "B", "content": "snippet b"}
                    ]
                }));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();
    // validate_url() rejects the loopback mock host without this.
    crate::core::http::set_allow_loopback(true);
    let hits = searxng_search(&cfg, "q", 10, None).await.expect("ok");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].url, "https://a.example/x");
    assert_eq!(hits[0].title, "A");
    assert_eq!(hits[0].snippet, "snippet a");
}

#[tokio::test]
async fn searxng_search_errors_when_json_format_disabled_403() {
    use crate::core::config::Config;
    use httpmock::MockServer;
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(403);
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();
    // validate_url() rejects the loopback mock host without this.
    crate::core::http::set_allow_loopback(true);
    let err = searxng_search(&cfg, "q", 10, None)
        .await
        .expect_err("403 should error");
    assert!(err.to_string().contains("error status"), "got: {err}");
}

#[tokio::test]
async fn searxng_search_filters_blank_urls_and_respects_count() {
    use crate::core::config::Config;
    use httpmock::MockServer;
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(200).json_body(serde_json::json!({
                "results": [
                    {"url": "", "title": "blank", "content": "x"},
                    {"url": "https://a/1", "title": "1", "content": "c1"},
                    {"url": "https://a/2", "title": "2", "content": "c2"}
                ]
            }));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();
    // validate_url() rejects the loopback mock host without this.
    crate::core::http::set_allow_loopback(true);
    let hits = searxng_search(&cfg, "q", 1, None).await.expect("ok");
    assert_eq!(hits.len(), 1, "blank url filtered, then take(1)");
    assert_eq!(hits[0].url, "https://a/1");
}
