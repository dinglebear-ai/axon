//! Unit tests for [`TavilySearchProvider`].
//!
//! `spider_agent::Agent`'s `with_search_tavily()` builder hardcodes Tavily's
//! live `https://api.tavily.com/search` endpoint with no way to inject a
//! mock URL through the public `AgentBuilder` API, so these tests can't
//! drive `search()` end-to-end against a fake HTTP server the way
//! `HttpFetchProvider`/`SearxngSearchProvider` do. Tavily's own wire
//! protocol is already `spider_agent`'s tested surface, not this crate's —
//! what this crate owns is (1) constructing the agent, (2) classifying
//! `spider_agent::AgentError` into the shared timeout/rate-limited/fatal
//! health buckets, and (3) mapping `SearchResults` into the boundary
//! [`SearchResult`] DTO. Those three are exactly what's tested here,
//! directly, without going over the network.

use axon_api::source::*;
use spider_agent::{AgentError, SearchError, SearchResult as TavilyResult, SearchResults};

use super::*;

#[tokio::test]
async fn new_builds_a_healthy_provider() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Healthy);
    assert!(capability.cooldown_until.is_none());
    assert_eq!(capability.provider_kind, ProviderKind::Search);
    assert_eq!(capability.implementation, "tavily");
}

#[test]
fn map_results_translates_tavily_hits_into_the_boundary_dto() {
    let mut raw = SearchResults::new("axon rust");
    raw.push(TavilyResult::new("Axon", "https://axon.test/", 1).with_snippet("a rust rag engine"));
    raw.push(TavilyResult::new("Axon docs", "https://axon.test/docs", 2));

    let mapped = map_results("axon rust".to_string(), raw);
    assert_eq!(mapped.query, "axon rust");
    assert_eq!(mapped.results.len(), 2);
    assert_eq!(mapped.results[0].url, "https://axon.test/");
    assert_eq!(mapped.results[0].snippet, "a rust rag engine");
    // No snippet in the source hit maps to an empty string, not a panic.
    assert_eq!(mapped.results[1].snippet, "");
}

#[tokio::test]
async fn rate_limited_errors_cool_the_provider_immediately() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    let err = provider
        .record_search_error(&AgentError::Search(SearchError::RateLimited))
        .await;
    assert_eq!(err.code.to_string(), "search.rate_limited");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(capability.reservation_state.available_units, 0);
}

#[tokio::test]
async fn top_level_rate_limited_variant_also_cools_the_provider() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    let err = provider.record_search_error(&AgentError::RateLimited).await;
    assert_eq!(err.code.to_string(), "search.rate_limited");
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Cooling
    );
}

#[tokio::test]
async fn timeout_errors_degrade_the_provider() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    let err = provider.record_search_error(&AgentError::Timeout).await;
    assert_eq!(err.code.to_string(), "search.timeout");
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Degraded
    );
}

#[tokio::test]
async fn authentication_failure_marks_the_provider_unavailable() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    let err = provider
        .record_search_error(&AgentError::Search(SearchError::AuthenticationFailed))
        .await;
    assert_eq!(err.code.to_string(), "search.fatal");
    assert!(!err.retryable);
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Unavailable
    );
}

#[tokio::test]
async fn a_success_recovers_a_previously_cooling_provider() {
    let provider = TavilySearchProvider::new("test-key").expect("agent build should not fail");
    provider.record_search_error(&AgentError::RateLimited).await;
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Cooling
    );

    provider.health.record_success().await;
    let recovered = provider.capabilities().await.expect("capabilities");
    assert_eq!(recovered.health, HealthStatus::Healthy);
    assert!(recovered.cooldown_until.is_none());
}
