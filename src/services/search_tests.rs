use super::*;
use crate::services::types::{ResearchPayload, ResearchTiming, ResearchUsage, SummarySource};
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
fn map_research_payload_wraps_payload() {
    let payload = ResearchPayload {
        query: "q".to_string(),
        limit: 1,
        offset: 0,
        search_results: vec![],
        extractions: vec![],
        summary: Some("s".to_string()),
        summary_source: SummarySource::Llm,
        usage: ResearchUsage::default(),
        timing_ms: ResearchTiming { total: 0 },
    };
    assert_eq!(map_research_payload(payload.clone()).payload, payload);
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

#[test]
fn redact_handles_kv_style_tokens() {
    // `?key=sk-…` was a known gap in the previous implementation: outer-trim
    // alone kept the prefix glued to `key=` and missed redaction.
    let cfg = Config::default();
    let summary = query_log_summary("debug request ?api_key=sk-livekey1234567890abc done", &cfg);
    assert!(
        summary.contains(REDACTED_TOKEN),
        "expected redaction: {summary}"
    );
    assert!(!summary.contains("sk-livekey1234567890abc"));
}

#[test]
fn redact_recognizes_aws_and_jwt_shapes() {
    let cfg = Config::default();
    let aws = query_log_summary("AKIAIOSFODNN7EXAMPLE configured", &cfg);
    assert!(
        aws.contains(REDACTED_TOKEN),
        "expected AWS redaction: {aws}"
    );
    let jwt = query_log_summary("Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig", &cfg);
    assert!(
        jwt.contains(REDACTED_TOKEN),
        "expected JWT redaction: {jwt}"
    );
}

#[test]
fn enforce_pagination_window_accepts_within_cap() {
    assert!(enforce_pagination_window(10, 0).is_ok());
    assert!(enforce_pagination_window(50, 50).is_ok());
    assert!(enforce_pagination_window(100, 0).is_ok());
}

#[test]
fn enforce_pagination_window_rejects_past_cap() {
    let err = enforce_pagination_window(60, 50).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("search window too large"), "got: {msg}");
    assert!(msg.contains("110"));
    assert!(msg.contains("100"));
}

#[test]
fn enforce_pagination_window_saturates_overflow() {
    // limit + offset overflow should still be rejected, not panic.
    assert!(enforce_pagination_window(usize::MAX, 1).is_err());
}

#[test]
fn ensure_tavily_configured_passes_with_key() {
    let cfg = Config {
        tavily_api_key: "tvly-key".to_string(),
        ..Config::default()
    };
    assert!(ensure_tavily_configured(&cfg, "research").is_ok());
}

#[test]
fn ensure_tavily_configured_rejects_empty_key() {
    let cfg = Config::default();
    let err = ensure_tavily_configured(&cfg, "research").unwrap_err();
    assert!(err.to_string().contains("TAVILY_API_KEY"));
    assert!(err.to_string().contains("research"));
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
