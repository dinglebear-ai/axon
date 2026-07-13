//! `SearxngSearchProvider` — a real, reqwest-backed [`SearchProvider`]
//! querying a self-hosted SearXNG instance's JSON API.
//!
//! Design choice (Wave 1a of issue #298, matching [`super::http_fetch`] /
//! [`super::chrome_render`]): same endpoint shape
//! (`{base_url}/search?format=json`), same SSRF guard
//! (`axon_core::http::validate_url` + the shared SSRF-guarded client
//! builder), and the same "walk pages until satisfied" pagination strategy
//! `axon-services`'s search backend used to hand-roll. **Wired into
//! `axon-services`** as of issue #298 WS-D: `crates/axon-services/src/search/provider.rs`
//! constructs this provider from `Config` and delegates `search`/`research`'s
//! SearXNG path to it (selected whenever `cfg.searxng_url` is set). Not yet
//! wired into `WebSourceAdapter`'s acquisition path — see the `providers`
//! module doc comment.

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::http::{axon_ua, build_ssrf_guarded_client_builder, validate_url};
use axon_error::ErrorStage;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use serde::Deserialize;

use crate::boundary::{Result, SearchProvider};

const PROVIDER_ID: &str = "searxng_search";

/// Max result pages to walk when satisfying a `limit` larger than one
/// SearXNG page (~10 results). Mirrors `axon-services`'s `MAX_SEARXNG_PAGES`.
const MAX_PAGES: usize = 10;

/// Self-tracked health/cooldown capacity — sized generously, purely to fold
/// live outcomes into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 2;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct SearxngSearchConfig {
    pub base_url: String,
    pub timeout: Duration,
}

impl Default for SearxngSearchConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearxngSearchProvider {
    config: SearxngSearchConfig,
    health: ProviderReservationManager,
}

#[derive(Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxRow>,
}

#[derive(Deserialize)]
struct SearxRow {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

impl SearxngSearchProvider {
    pub fn new(config: SearxngSearchConfig) -> Self {
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Search,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        Self { config, health }
    }

    pub fn config(&self) -> &SearxngSearchConfig {
        &self.config
    }

    fn error(&self, code: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(code, ErrorStage::Discovering, message.into()).with_provider_id(PROVIDER_ID)
    }

    fn endpoint(&self) -> std::result::Result<String, ApiError> {
        let endpoint = format!("{}/search", self.config.base_url.trim_end_matches('/'));
        validate_url(&endpoint).map_err(|err| self.error("search.invalid_url", err.to_string()))?;
        Ok(endpoint)
    }

    async fn record_timeout(&self) {
        self.health.record_failure("provider.timeout", true).await;
    }

    /// A 429/format-disabled-style rate limit is a maximal-severity signal:
    /// recorded as two strikes so cooldown-after-2 trips on the very first
    /// occurrence. Mirrors `HttpFetchProvider::record_rate_limited`.
    async fn record_rate_limited(&self) {
        for _ in 0..HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES {
            self.health
                .record_failure("provider.rate_limited", true)
                .await;
        }
    }

    async fn record_fatal(&self) {
        self.health.record_failure("provider.fatal", false).await;
    }

    /// Fetch and decode one SearXNG results page. Split out of `search()` to
    /// keep that function under the monolith function-length warning.
    async fn fetch_page(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        query: &str,
        pageno: usize,
        time_range: Option<&'static str>,
    ) -> Result<Vec<SearxRow>> {
        let mut params: Vec<(&str, String)> = vec![
            ("q", query.to_string()),
            ("format", "json".to_string()),
            ("pageno", pageno.to_string()),
        ];
        if let Some(tr) = time_range {
            params.push(("time_range", tr.to_string()));
        }
        let response = match client
            .get(endpoint)
            .query(&params)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) if err.is_timeout() => {
                self.record_timeout().await;
                return Err(self.error("search.timeout", "request timed out"));
            }
            Err(err) => {
                self.record_fatal().await;
                return Err(self.error("search.transport", err.to_string()));
            }
        };

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            self.record_rate_limited().await;
            return Err(self.error("search.rate_limited", "provider returned HTTP 429"));
        }
        if !status.is_success() {
            self.record_fatal().await;
            return Err(self.error(
                "search.bad_status",
                format!("provider returned HTTP {}", status.as_u16()),
            ));
        }

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                self.record_fatal().await;
                return Err(self.error("search.body_read", err.to_string()));
            }
        };
        let parsed: SearxResponse = match serde_json::from_slice(&bytes) {
            Ok(parsed) => parsed,
            Err(err) => {
                // A non-JSON 200 body most commonly means the `json` output
                // format is disabled in the instance's `settings.yml` —
                // treated as fatal (non-retryable): retrying won't fix a
                // config error.
                self.record_fatal().await;
                return Err(self.error(
                    "search.decode",
                    format!(
                        "searxng JSON decode failed (is the `json` output format enabled in settings.yml?): {err}"
                    ),
                ));
            }
        };
        Ok(parsed.results)
    }
}

#[async_trait]
impl SearchProvider for SearxngSearchProvider {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult> {
        let endpoint = self.endpoint()?;
        let client = build_ssrf_guarded_client_builder(Some(self.config.timeout))
            .user_agent(axon_ua())
            .build()
            .map_err(|err| self.error("search.client_init", err.to_string()))?;

        let limit = (request.limit as usize).max(1);
        let offset = request.offset as usize;
        // Fetch `offset + limit` unique hits, then window below — SearXNG has
        // no offset param of its own, so the caller-visible window is applied
        // client-side after cross-page dedup.
        let total = limit.saturating_add(offset).max(1);
        let time_range = request.time_range.map(time_range_param);
        let mut items: Vec<SearchResultItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for pageno in 1..=MAX_PAGES {
            if items.len() >= total {
                break;
            }
            let rows = self
                .fetch_page(&client, &endpoint, &request.query, pageno, time_range)
                .await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                if items.len() >= total {
                    break;
                }
                if row.url.is_empty() || !seen.insert(row.url.clone()) {
                    continue;
                }
                items.push(SearchResultItem {
                    title: row.title,
                    url: row.url,
                    snippet: row.content,
                });
            }
        }

        self.health.record_success().await;
        Ok(SearchResult {
            query: request.query,
            results: items.into_iter().skip(offset).take(limit).collect(),
        })
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
            implementation: "searxng".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health,
            limits: ProviderLimits {
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                ..ProviderLimits::default()
            },
            features: vec![
                "json".to_string(),
                "pagination".to_string(),
                "offset".to_string(),
                "time_range".to_string(),
            ],
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
            cost_class: ProviderCostClass::Internal,
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

/// Map the boundary [`SearchTimeRange`] to SearXNG's `time_range` query value.
fn time_range_param(tr: SearchTimeRange) -> &'static str {
    match tr {
        SearchTimeRange::Day => "day",
        SearchTimeRange::Week => "week",
        SearchTimeRange::Month => "month",
        SearchTimeRange::Year => "year",
    }
}

#[cfg(test)]
#[path = "searxng_search_tests.rs"]
mod tests;
