//! `TavilySearchProvider` — a real [`SearchProvider`] wrapping
//! `spider_agent`'s built-in Tavily AI Search client
//! (`spider_agent::Agent::builder().with_search_tavily(api_key)`), the same
//! client `axon-services`'s `search`/`research` Tavily path already uses
//! (see `crates/axon-services/src/search.rs` and
//! `crates/axon-services/src/search/synthesis.rs`). This reuses
//! `spider_agent`'s HTTP handling and JSON parsing directly rather than
//! hand-rolling a second Tavily client.
//!
//! Design choice (Wave 1a of issue #298, matching [`super::http_fetch`] /
//! [`super::chrome_render`] / [`super::searxng_search`]): not yet wired into
//! `axon-services` or `WebSourceAdapter` — see the `providers` module doc
//! comment for the same scaffolding note that applies to the other real
//! providers here.

use async_trait::async_trait;
use axon_api::source::*;
use axon_error::ErrorStage;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use spider_agent::{Agent, AgentError, SearchError, SearchOptions as TavilySearchOptions};

use crate::boundary::{Result, SearchProvider};

const PROVIDER_ID: &str = "tavily_search";

/// Self-tracked health/cooldown capacity — sized generously, purely to fold
/// live outcomes into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 2;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

pub struct TavilySearchProvider {
    agent: Agent,
    health: ProviderReservationManager,
}

impl TavilySearchProvider {
    /// Build a provider from a Tavily API key. `spider_agent::Agent::build()`
    /// performs no network I/O, so this only fails on malformed agent
    /// configuration.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let agent = Agent::builder()
            .with_search_tavily(api_key)
            .build()
            .map_err(|err| {
                ApiError::new(
                    "search.client_init",
                    ErrorStage::Discovering,
                    err.to_string(),
                )
                .with_provider_id(PROVIDER_ID)
            })?;
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Search,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        Ok(Self { agent, health })
    }

    fn error(&self, code: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(code, ErrorStage::Discovering, message.into()).with_provider_id(PROVIDER_ID)
    }

    /// Classify a `spider_agent` search failure into the same
    /// timeout/rate-limited/fatal buckets `HttpFetchProvider` uses, folding
    /// the outcome into the shared health tracker.
    async fn record_search_error(&self, err: &AgentError) -> ApiError {
        match err {
            AgentError::RateLimited | AgentError::Search(SearchError::RateLimited) => {
                for _ in 0..HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES {
                    self.health
                        .record_failure("provider.rate_limited", true)
                        .await;
                }
                self.error("search.rate_limited", err.to_string())
            }
            AgentError::Timeout => {
                self.health.record_failure("provider.timeout", true).await;
                self.error("search.timeout", err.to_string())
            }
            AgentError::Http(reqwest_err) if reqwest_err.is_timeout() => {
                self.health.record_failure("provider.timeout", true).await;
                self.error("search.timeout", err.to_string())
            }
            AgentError::Http(_) | AgentError::Search(SearchError::RequestFailed(_)) => {
                self.health
                    .record_failure("provider.transport", true)
                    .await;
                self.error("search.transport", err.to_string())
            }
            _ => {
                self.health.record_failure("provider.fatal", false).await;
                self.error("search.fatal", err.to_string())
            }
        }
    }
}

/// Map `spider_agent`'s unified [`spider_agent::SearchResults`] into the
/// boundary [`SearchResult`] DTO. Split out of `search()` so the mapping
/// (the logic actually owned by this crate, as opposed to the Tavily wire
/// protocol spider_agent itself already tests) has a unit test that doesn't
/// require network access.
fn map_results(query: String, results: spider_agent::SearchResults) -> SearchResult {
    SearchResult {
        query,
        results: results
            .results
            .into_iter()
            .map(|r| SearchResultItem {
                title: r.title,
                url: r.url,
                snippet: r.snippet.unwrap_or_default(),
            })
            .collect(),
    }
}

#[async_trait]
impl SearchProvider for TavilySearchProvider {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult> {
        let options = TavilySearchOptions::new().with_limit((request.limit as usize).max(1));
        let results = match self
            .agent
            .search_with_options(&request.query, options)
            .await
        {
            Ok(results) => results,
            Err(err) => return Err(self.record_search_error(&err).await),
        };

        self.health.record_success().await;
        Ok(map_results(request.query, results))
    }

    /// Reports the provider's **live** health/cooldown, folded in from every
    /// [`search`](Self::search) call's outcome — mirrors
    /// `HttpFetchProvider::capabilities`.
    async fn capabilities(&self) -> Result<ProviderCapability> {
        let health = self.health.health().await;
        let cooldown_until = self.health.cooldown_until().await;
        let last_error = self
            .health
            .cooling_snapshot()
            .await
            .map(|cooling| self.error("provider.cooling", cooling.reason));
        Ok(ProviderCapability {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Search,
            implementation: "tavily".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health,
            limits: ProviderLimits::default(),
            features: vec!["ai_search".to_string()],
            cooldown_until,
            last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Fifo,
                interactive_reserve: 0,
                cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
                cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
                retry_backoff_ms: None,
            },
            reservation_state: super::single_slot_reservation_state(health),
            cost_class: ProviderCostClass::Standard,
            degraded_modes: Vec::new(),
            fake_overrides_supported: false,
            embedding: None,
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        })
    }
}

#[cfg(test)]
#[path = "tavily_search_tests.rs"]
mod tests;
