use super::*;
use serde_json::json;

#[test]
fn time_range_all_variants_map_correctly() {
    assert_eq!(to_spider_time_range(ServiceTimeRange::Day), TimeRange::Day);
    assert_eq!(
        to_spider_time_range(ServiceTimeRange::Week),
        TimeRange::Week
    );
    assert_eq!(
        to_spider_time_range(ServiceTimeRange::Month),
        TimeRange::Month
    );
    assert_eq!(
        to_spider_time_range(ServiceTimeRange::Year),
        TimeRange::Year
    );
}

#[test]
fn map_search_results_empty_vec() {
    assert!(map_search_results(vec![]).results.is_empty());
}

#[test]
fn map_search_results_nonempty() {
    let item = json!({"title": "Axon docs", "url": "https://example.com"});
    let result = map_search_results(vec![item.clone()]);
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0], item);
}

#[test]
fn map_research_payload_stores_value() {
    let value = json!({"answer": "42", "sources": []});
    assert_eq!(map_research_payload(value.clone()).payload, value);
}

#[test]
fn query_log_summary_redacts_token_like_substrings() {
    let cfg = Config::default();
    let summary = query_log_summary(
        "find docs for sk-testsecret1234567890 and github_pat_1234567890abcdef",
        &cfg,
    );
    assert!(summary.contains("len="));
    assert!(summary.contains("hash="));
    assert!(summary.contains(REDACTED_TOKEN));
    assert!(!summary.contains("sk-testsecret1234567890"));
    assert!(!summary.contains("github_pat_1234567890abcdef"));
}

#[tokio::test]
async fn search_batch_empty_queries_returns_empty() {
    let cfg = Config::default();
    let result = search_batch(
        &cfg,
        &[],
        SearchOptions {
            limit: 10,
            offset: 0,
            time_range: None,
        },
        None,
    )
    .await
    .expect("search_batch with empty queries should not fail");
    assert!(result.results.is_empty());
}
