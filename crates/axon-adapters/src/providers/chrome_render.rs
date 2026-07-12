//! `ChromeRenderProvider` — a real [`RenderProvider`] wrapping axon-crawl's
//! single-page Spider transport.
//!
//! Design choice (Wave 1a of issue #298): rendering (turning a URI into
//! markdown/HTML, optionally via a headless browser) is exactly what
//! `axon_crawl::scrape::scrape_to_result` already does — HTTP-first with
//! Chrome fallback, SSRF-guarded, thin-page detection included. Reimplementing
//! that here would duplicate a large, already-hardened surface (Spider
//! `Website` config, ETag caching, sitemap-aware retries). `axon-crawl` is a
//! temporary dependency of this crate for exactly this wrapper — Wave 2 of
//! #298 relocates the crawl engine itself and this dependency goes away (see
//! the Cargo.toml comment beside it).
//!
//! `cfg.format` is pinned to `Html` for every render: `ScrapeResult.output`
//! then carries the raw HTML while `ScrapeResult.markdown` (always populated
//! independent of `format`) carries the markdown conversion, so one
//! `scrape_to_result` call fills both `RenderedResource.html` and `.markdown`.

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::config::{Config, RenderMode as CoreRenderMode, ScrapeFormat};
use axon_core::logging::log_warn;
use axon_error::ErrorStage;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use chrono::Utc;

use crate::boundary::{RenderProvider, Result};

const PROVIDER_ID: &str = "chrome_render";

/// Self-tracked health/cooldown capacity — sized generously, purely to fold
/// live outcomes into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;

/// Mirrors `HttpFetchProvider`'s threshold: a single retryable failure (e.g.
/// one timeout) reports `Degraded`; a rate-limited response is recorded as
/// two strikes so it reaches `Cooling` with a `cooldown_until` on the first
/// occurrence rather than requiring two consecutive ones.
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 2;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

#[derive(Debug, Clone, Default)]
pub struct ChromeRenderConfig {
    /// Overrides `Config::default().chrome_remote_url` (the CDP endpoint) for
    /// every render — e.g. `http://axon-chrome:6000`. `None` leaves axon-crawl
    /// to fall back to a locally-launched Chrome, matching CLI defaults.
    pub chrome_remote_url: Option<String>,
    /// Fallback request timeout applied when a [`RenderRequest`] does not set
    /// its own `timeout_ms`.
    pub default_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ChromeRenderProvider {
    config: ChromeRenderConfig,
    health: ProviderReservationManager,
}

impl ChromeRenderProvider {
    pub fn new(config: ChromeRenderConfig) -> Self {
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Render,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        Self { config, health }
    }

    pub fn config(&self) -> &ChromeRenderConfig {
        &self.config
    }

    fn error(&self, code: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(code, ErrorStage::Rendering, message.into()).with_provider_id(PROVIDER_ID)
    }

    /// Build the `axon-core` `Config` `scrape_to_result` needs for one
    /// render, seeded from `Config::default()` (see the crate doc's "Adding
    /// fields to `Config`" note — this is the single supported way to obtain
    /// a valid `Config`) with only the render-relevant fields overridden.
    fn build_config(&self, request: &RenderRequest) -> Config {
        let mut cfg = Config {
            render_mode: map_render_mode(request.mode),
            format: ScrapeFormat::Html,
            request_timeout_ms: request.timeout_ms.or(self.config.default_timeout_ms),
            ..Config::default()
        };
        if let Some(remote_url) = &self.config.chrome_remote_url {
            cfg.chrome_remote_url = Some(remote_url.clone());
        }
        cfg
    }
}

pub(crate) fn map_render_mode(mode: RenderMode) -> CoreRenderMode {
    match mode {
        RenderMode::Http => CoreRenderMode::Http,
        RenderMode::Chrome => CoreRenderMode::Chrome,
        RenderMode::AutoSwitch => CoreRenderMode::AutoSwitch,
    }
}

pub(crate) fn map_core_render_mode(mode: CoreRenderMode) -> RenderMode {
    match mode {
        CoreRenderMode::Http => RenderMode::Http,
        CoreRenderMode::Chrome => RenderMode::Chrome,
        CoreRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

/// Classification of a `scrape_to_result` failure, derived from its
/// `Box<dyn Error>` message text — the underlying axon-crawl error carries no
/// typed status to match on (unlike `HttpFetchProvider`, which classifies a
/// real `reqwest::StatusCode`). Mirrors the same three-way health mapping: a
/// transient timeout is `Degraded`, a rate-limited response is `Cooling`,
/// everything else (5xx, connection failure, SSRF rejection) is `Unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderFailureClass {
    Timeout,
    RateLimited,
    Fatal,
}

pub(crate) fn classify_render_error(message: &str) -> RenderFailureClass {
    let lower = message.to_ascii_lowercase();
    if lower.contains("http 429") || lower.contains("rate limit") {
        RenderFailureClass::RateLimited
    } else if lower.contains("timeout") || lower.contains("timed out") {
        RenderFailureClass::Timeout
    } else {
        RenderFailureClass::Fatal
    }
}

#[async_trait]
impl RenderProvider for ChromeRenderProvider {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource> {
        if request.automation_script.is_some() {
            return Err(self.error(
                "render.automation_script_unsupported",
                "automation scripts are not yet supported by the single-page render provider",
            ));
        }

        let mut cfg = self.build_config(&request);
        if axon_crawl::chrome_bootstrap::chrome_runtime_requested(&cfg) {
            let bootstrap = axon_crawl::chrome_bootstrap::bootstrap_chrome_runtime(&cfg).await;
            for warning in &bootstrap.warnings {
                log_warn(&format!("[chrome_render] {warning}"));
            }
            if let Some(ws_url) = bootstrap.resolved_ws_url {
                cfg.chrome_remote_url = Some(ws_url);
            }
        }
        let render_mode = cfg.render_mode;

        // `Box<dyn Error>` (axon-crawl's error type) is not `Send`, so it must
        // not live across an `.await` — convert to an owned `String` (`Send`)
        // immediately, synchronously, right after the outer await resolves.
        let outcome = axon_crawl::scrape::scrape_to_result(&cfg, &request.uri)
            .await
            .map_err(|err| err.to_string());

        match outcome {
            Ok(result) => {
                self.health.record_success().await;
                Ok(RenderedResource {
                    uri: request.uri,
                    final_uri: result.url,
                    markdown: result.markdown,
                    html: Some(result.output),
                    text: None,
                    render_mode: map_core_render_mode(render_mode),
                    captured_at: Timestamp::from(Utc::now()),
                    artifacts: Vec::new(),
                    console: Vec::new(),
                    network: Vec::new(),
                    metadata: request.metadata,
                })
            }
            Err(message) => {
                match classify_render_error(&message) {
                    RenderFailureClass::Timeout => {
                        self.health.record_failure("render.timeout", true).await;
                        Err(self.error("render.timeout", message))
                    }
                    RenderFailureClass::RateLimited => {
                        for _ in 0..HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES {
                            self.health
                                .record_failure("render.rate_limited", true)
                                .await;
                        }
                        Err(self.error("render.rate_limited", message))
                    }
                    RenderFailureClass::Fatal => {
                        self.health.record_failure("render.fatal", false).await;
                        Err(self.error("render.fatal", message))
                    }
                }
            }
        }
    }

    /// Reports the provider's **live** health/cooldown, folded in from every
    /// [`render`](Self::render) call's outcome — mirrors
    /// `axon-embedding`'s `TeiEmbeddingProvider::capabilities`.
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
            provider_kind: ProviderKind::Render,
            implementation: "axon-crawl-spider".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health,
            limits: ProviderLimits {
                timeout_ms: self.config.default_timeout_ms,
                ..ProviderLimits::default()
            },
            features: vec!["html".to_string(), "markdown".to_string()],
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
            render: Some(RenderProviderCapability {
                render_modes: vec![RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch],
                browser_pool_limits: BrowserPoolLimits {
                    max_browsers: 1,
                    max_pages_per_browser: 1,
                    max_page_lifetime_ms: self.config.default_timeout_ms.unwrap_or(30_000),
                },
                script_support: false,
            }),
            credential: None,
        })
    }
}

#[cfg(test)]
#[path = "chrome_render_tests.rs"]
mod tests;
