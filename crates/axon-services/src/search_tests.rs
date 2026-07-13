use super::*;
use crate::types::{ResearchPayload, ResearchTiming, ResearchUsage, SummarySource};
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
        auto_crawl_status: "not_queued".to_string(),
        crawl_jobs: vec![],
        crawl_jobs_rejected: vec![],
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
    let fake_key = ["sk-", "livekey1234567890abc"].concat();
    let summary = query_log_summary(&format!("debug request ?api_key={fake_key} done"), &cfg);
    assert!(
        summary.contains(REDACTED_TOKEN),
        "expected redaction: {summary}"
    );
    assert!(!summary.contains(&fake_key));
}

#[test]
fn redact_recognizes_aws_and_jwt_shapes() {
    let cfg = Config::default();
    let fake_aws = ["AKIA", "IOSFODNN7EXAMPLE"].concat();
    let aws = query_log_summary(&format!("{fake_aws} configured"), &cfg);
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

// ── search_results delegation ───────────────────────────────────────────────

mod search_results_delegation_tests {
    use super::*;
    use axon_core::http::LoopbackGuard;
    use httpmock::MockServer;

    fn searx_json(results: &[(&str, &str, &str)]) -> serde_json::Value {
        serde_json::json!({
            "results": results
                .iter()
                .map(|(url, title, content)| {
                    serde_json::json!({ "url": url, "title": title, "content": content })
                })
                .collect::<Vec<_>>()
        })
    }

    /// `search_results` positions must be `offset + i + 1`, recomputed after
    /// delegating the fetch/window step to `SearxngSearchProvider`.
    #[tokio::test]
    async fn search_results_recomputes_position_from_offset() {
        let _loopback = LoopbackGuard::allow();
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/search");
                then.status(200).json_body(searx_json(&[
                    ("https://a.test/1", "1", "c1"),
                    ("https://a.test/2", "2", "c2"),
                    ("https://a.test/3", "3", "c3"),
                ]));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.searxng_url = server.base_url();

        let results = search_results(&cfg, "q", 1, 2, None)
            .await
            .expect("search_results should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["position"], 3);
        assert_eq!(results[0]["url"], "https://a.test/3");
    }

    /// The 100-result pagination cap only applies to the Tavily fallback path
    /// — SearXNG walks its own pages internally and is not capped here. This
    /// asymmetry predates the provider delegation; this test guards against
    /// losing it.
    #[tokio::test]
    async fn search_results_pagination_cap_not_enforced_for_searxng() {
        let _loopback = LoopbackGuard::allow();
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/search");
                then.status(200)
                    .json_body(searx_json(&[("https://a.test/1", "1", "c1")]));
            })
            .await;
        let mut cfg = Config::test_default();
        cfg.searxng_url = server.base_url();

        // limit=60 + offset=50 = 110 > SEARCH_WINDOW_MAX(100): this would be
        // rejected on the Tavily path but must succeed here.
        let results = search_results(&cfg, "q", 60, 50, None)
            .await
            .expect("searxng path must not enforce the tavily-only pagination cap");
        assert!(results.len() <= 1);
    }

    /// The same oversized window IS rejected on the Tavily fallback path,
    /// before any network call (fails on the pagination-window check itself).
    #[tokio::test]
    async fn search_results_pagination_cap_enforced_for_tavily_fallback() {
        let mut cfg = Config::test_default();
        cfg.tavily_api_key = "tvly-test-key".to_string();
        assert!(cfg.searxng_url.is_empty());

        let err = search_results(&cfg, "q", 60, 50, None)
            .await
            .expect_err("110 > SEARCH_WINDOW_MAX must be rejected");
        assert!(err.to_string().contains("search window too large"));
    }
}
