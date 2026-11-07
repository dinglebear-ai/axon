//! `ChromeRenderProvider` — a real [`RenderProvider`] wrapping the in-crate
//! `web_engine` single-page Spider transport.
//!
//! Design choice (Wave 1a of issue #298): rendering (turning a URI into
//! markdown/HTML, optionally via a headless browser) is exactly what
//! `crate::web_engine::scrape::scrape_to_result` already does — HTTP-first
//! with Chrome fallback, SSRF-guarded, thin-page detection included.
//! Reimplementing that here would duplicate a large, already-hardened surface
//! (Spider `Website` config, ETag caching, sitemap-aware retries). The former
//! `axon-crawl` crate was a temporary dependency of this crate for exactly
//! this wrapper — Wave 2a of #298 relocated the crawl engine into
//! `crate::web_engine` and deleted that crate (see `crates/axon-adapters/src/web_engine.rs`).
//!
//! `cfg.format` is pinned to `Html` for every render: `ScrapeResult.output`
//! then carries the raw HTML while `ScrapeResult.markdown` (always populated
//! independent of `format`) carries the markdown conversion, so one
//! `scrape_to_result` call fills both `RenderedResource.html` and `.markdown`.
//!
//! `RenderRequest.automation_script` (issue #298 Wave 2b regression 1,
//! restoring a Wave 1a stub that unconditionally rejected it): resolved here
//! into `Config::automation_script` and executed by
//! `web_engine::scrape::scrape_to_result` — see that module's
//! `apply_automation_scripts` for the actual Chrome-only execution gate.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::config::{Config, RenderMode as CoreRenderMode, ScrapeFormat};
use axon_core::http::validate_url_with_dns;
use axon_core::logging::log_warn;
use axon_error::ErrorStage;

static TIMED_OUT_RENDER_TASKS_AWAITING_REAP: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
const RENDER_REAPER_OBSERVATION_GRACE: StdDuration = StdDuration::from_millis(10);
#[cfg(not(test))]
const RENDER_REAPER_OBSERVATION_GRACE: StdDuration = StdDuration::from_secs(30);
use axon_error::{RetryPolicy, RetryScope};
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use chrono::Utc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::boundary::{RenderProvider, Result};

pub const CHROME_RENDER_PROVIDER_ID: &str = "chrome_render";
const PROVIDER_ID: &str = CHROME_RENDER_PROVIDER_ID;

/// Self-tracked health/cooldown capacity — sized generously, purely to fold
/// live outcomes into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;

/// Mirrors `HttpFetchProvider`'s threshold: a single retryable failure (e.g.
/// one timeout) reports `Degraded`; a rate-limited response is recorded as
/// two strikes so it reaches `Cooling` with a `cooldown_until` on the first
/// occurrence rather than requiring two consecutive ones.
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 2;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;
const REMOTE_CHROME_MAX_CONCURRENT_PAGES: u32 = 8;

#[derive(Debug, Clone, Default)]
pub struct ChromeRenderConfig {
    /// Maximum in-flight rendered pages for this provider instance.
    pub max_concurrent_pages: Option<usize>,
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
    page_slots: Arc<Semaphore>,
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
        let max_concurrent_pages = config
            .max_concurrent_pages
            .unwrap_or(REMOTE_CHROME_MAX_CONCURRENT_PAGES as usize)
            .max(1);
        Self {
            config,
            health,
            page_slots: Arc::new(Semaphore::new(max_concurrent_pages)),
        }
    }

    pub fn config(&self) -> &ChromeRenderConfig {
        &self.config
    }

    async fn acquire_page_slot(&self) -> std::result::Result<OwnedSemaphorePermit, ApiError> {
        self.page_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| self.error("render.capacity_closed", "Chrome page capacity is closed"))
    }

    async fn acquire_page_slot_for(
        &self,
        mode: CoreRenderMode,
        deadline: tokio::time::Instant,
    ) -> std::result::Result<Option<OwnedSemaphorePermit>, ApiError> {
        if matches!(mode, CoreRenderMode::Chrome | CoreRenderMode::AutoSwitch) {
            tokio::time::timeout_at(deadline, self.acquire_page_slot())
                .await
                .map_err(|_| {
                    self.error(
                        "render.timeout",
                        "render timed out while waiting for Chrome page capacity",
                    )
                    .with_retry_policy(RetryPolicy::retryable(RetryScope::Item))
                })?
                .map(Some)
        } else {
            Ok(None)
        }
    }

    fn error(&self, code: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(code, ErrorStage::Rendering, message.into()).with_provider_id(PROVIDER_ID)
    }

    /// Build the `axon-core` `Config` `scrape_to_result` needs for one
    /// render, seeded from `Config::default()` (see the crate doc's "Adding
    /// fields to `Config`" note — this is the single supported way to obtain
    /// a valid `Config`) with only the render-relevant fields overridden.
    fn build_config(&self, request: &RenderRequest) -> Config {
        let bool_value = |key| {
            request
                .metadata
                .get(key)
                .and_then(serde_json::Value::as_bool)
        };
        let string_value = |key| {
            request
                .metadata
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let u64_value = |key| {
            request
                .metadata
                .get(key)
                .and_then(serde_json::Value::as_u64)
        };
        let mut cfg = Config {
            render_mode: map_render_mode(request.mode),
            format: request
                .metadata
                .get("format")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or(ScrapeFormat::Html),
            request_timeout_ms: request.timeout_ms.or(self.config.default_timeout_ms),
            normalize: bool_value("normalize").unwrap_or(false),
            block_assets: bool_value("block_assets").unwrap_or(false),
            chrome_wait_for_selector: string_value("chrome_wait_for_selector"),
            root_selector: string_value("root_selector"),
            exclude_selector: string_value("exclude_selector"),
            chrome_screenshot: bool_value("chrome_screenshot").unwrap_or(false),
            chrome_network_idle_timeout_secs: u64_value("chrome_network_idle_timeout_secs")
                .unwrap_or_else(|| Config::default().chrome_network_idle_timeout_secs),
            automation_script: request
                .automation_script
                .as_ref()
                .map(|artifact| automation_script_path(&artifact.uri)),
            ..Config::default()
        };
        if let Some(remote_url) = &self.config.chrome_remote_url {
            cfg.chrome_remote_url = Some(remote_url.clone());
        }
        if let Some(output_dir) = string_value("output_dir") {
            cfg.output_dir = output_dir.into();
        }
        cfg
    }
}

/// `RenderRequest.automation_script.uri` carries a local filesystem path (see
/// `web::options::automation_script_ref`, which is the only current
/// constructor for this field). Support an optional `file://` prefix
/// defensively since `ArtifactRef.uri` is documented as a URI, not
/// specifically a bare path.
fn automation_script_path(uri: &str) -> PathBuf {
    PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri))
}

async fn await_isolated_render_outcome<T, F>(
    timeout: StdDuration,
    future: F,
) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let (cancel, canceled) = tokio::sync::oneshot::channel();
    #[cfg(test)]
    let allow_loopback = axon_core::http::get_allow_loopback();
    let mut task = tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        let _loopback = axon_core::http::LoopbackGuard::set(allow_loopback);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("render runtime failed: {error}"))?;
        runtime.block_on(async move {
            tokio::select! {
                outcome = future => Ok(outcome),
                _ = canceled => Err("render task canceled after deadline".to_string()),
            }
        })
    });
    match tokio::time::timeout(timeout, &mut task).await {
        Ok(Ok(Ok(outcome))) => Ok(outcome),
        Ok(Ok(Err(error))) => Err(error),
        Ok(Err(error)) => Err(format!("render task failed: {error}")),
        Err(_) => {
            let _ = cancel.send(());
            spawn_render_reaper(task);
            Err(format!("render timed out after {}ms", timeout.as_millis()))
        }
    }
}

fn spawn_render_reaper<T>(mut task: tokio::task::JoinHandle<std::result::Result<T, String>>)
where
    T: Send + 'static,
{
    TIMED_OUT_RENDER_TASKS_AWAITING_REAP.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        match tokio::time::timeout(RENDER_REAPER_OBSERVATION_GRACE, &mut task).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "timed-out render task reaper failed");
            }
            Err(_) => {
                tracing::warn!(
                    pending_timed_out_renders =
                        TIMED_OUT_RENDER_TASKS_AWAITING_REAP.load(Ordering::Relaxed),
                    "timed-out render task remains blocked after cleanup grace; Chrome capacity may remain reduced until the underlying operation returns"
                );
                if let Err(error) = task.await {
                    tracing::warn!(%error, "timed-out render task reaper failed");
                }
            }
        }
        TIMED_OUT_RENDER_TASKS_AWAITING_REAP.fetch_sub(1, Ordering::Relaxed);
    });
}

impl ChromeRenderProvider {
    async fn render_inner(
        request: RenderRequest,
        mut cfg: Config,
        timeout_policy: crate::web_engine::browser::BrowserTimeoutPolicy,
    ) -> std::result::Result<RenderedResource, String> {
        if crate::web_engine::chrome_bootstrap::chrome_runtime_requested(&cfg) {
            let bootstrap =
                crate::web_engine::chrome_bootstrap::bootstrap_chrome_runtime(&cfg).await;
            for warning in &bootstrap.warnings {
                log_warn(&format!("[chrome_render] {warning}"));
            }
            crate::web_engine::chrome_bootstrap::apply_bootstrap_outcome(&mut cfg, &bootstrap);
        }
        let render_mode = cfg.render_mode;
        crate::web_engine::scrape::scrape_to_result_with_http_outcomes(
            &cfg,
            &request.uri,
            timeout_policy,
        )
        .await
        .map_err(|err| err.to_string())
        .map(|result| {
            let mut metadata = request.metadata;
            metadata.remove("web_title");
            metadata.remove("web_status");
            if let Some(status) = result
                .payload
                .get("status_code")
                .and_then(serde_json::Value::as_u64)
            {
                metadata.insert("web_status".into(), serde_json::json!(status));
            }
            if let Some(title) = result
                .payload
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
            {
                metadata.insert("web_title".to_string(), serde_json::json!(title));
            }
            RenderedResource {
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
                metadata,
            }
        })
    }
}

#[path = "chrome_render/modes.rs"]
mod modes;
pub(crate) use modes::{map_core_render_mode, map_render_mode};

#[path = "chrome_render/failure.rs"]
mod failure;
pub(crate) use failure::{RenderFailureClass, classify_render_error};

#[async_trait]
impl RenderProvider for ChromeRenderProvider {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource> {
        let cfg = self.build_config(&request);
        let timeout_policy = if request
            .metadata
            .get("exact_browser_timeout")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            crate::web_engine::browser::BrowserTimeoutPolicy::Exact
        } else {
            crate::web_engine::browser::BrowserTimeoutPolicy::FloorForBrowserWork
        };

        let browser_timeout_ms = cfg
            .chrome_network_idle_timeout_secs
            .saturating_add(30)
            .saturating_mul(1_000);
        let request_timeout_ms = match (cfg.request_timeout_ms, timeout_policy) {
            (Some(timeout_ms), crate::web_engine::browser::BrowserTimeoutPolicy::Exact) => {
                timeout_ms
            }
            (Some(timeout_ms), _) => timeout_ms.max(browser_timeout_ms),
            (None, _) => browser_timeout_ms,
        };
        let render_deadline = StdDuration::from_millis(request_timeout_ms.saturating_add(5_000));
        let now = tokio::time::Instant::now();
        let absolute_deadline = now
            .checked_add(render_deadline)
            .unwrap_or_else(|| now + StdDuration::from_secs(365 * 24 * 60 * 60));
        tokio::time::timeout_at(absolute_deadline, validate_url_with_dns(&request.uri))
            .await
            .map_err(|_| {
                self.error(
                    "render.timeout",
                    "render timed out while validating the target address",
                )
                .with_retry_policy(RetryPolicy::retryable(RetryScope::Item))
            })?
            .map_err(|err| {
                self.error(
                    "render.invalid_uri",
                    format!("render target rejected by SSRF policy: {err}"),
                )
            })?;
        let page_permit = self
            .acquire_page_slot_for(cfg.render_mode, absolute_deadline)
            .await?;
        let remaining = absolute_deadline.saturating_duration_since(tokio::time::Instant::now());
        let outcome = await_isolated_render_outcome(remaining, async move {
            let _page_permit = page_permit;
            ChromeRenderProvider::render_inner(request, cfg, timeout_policy).await
        })
        .await
        .and_then(|outcome| outcome);

        match outcome {
            Ok(result) => {
                self.health.record_success().await;
                Ok(result)
            }
            Err(message) => match classify_render_error(&message) {
                RenderFailureClass::Timeout => {
                    self.health.record_failure("render.timeout", true).await;
                    Err(self
                        .error("render.timeout", message)
                        .with_retry_policy(RetryPolicy::retryable(RetryScope::Item)))
                }
                RenderFailureClass::RateLimited => {
                    for _ in 0..HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES {
                        self.health
                            .record_failure("render.rate_limited", true)
                            .await;
                    }
                    Err(self
                        .error("render.rate_limited", message)
                        .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider)))
                }
                RenderFailureClass::Transient => {
                    self.health.record_failure("render.transient", true).await;
                    Err(self
                        .error("render.transient", message)
                        .with_retry_policy(RetryPolicy::retryable(RetryScope::Item)))
                }
                RenderFailureClass::Fatal => {
                    self.health.record_failure("render.fatal", false).await;
                    Err(self.error("render.fatal", message))
                }
            },
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
            features: vec![
                "html".to_string(),
                "markdown".to_string(),
                "automation_script".to_string(),
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
                    max_pages_per_browser: REMOTE_CHROME_MAX_CONCURRENT_PAGES,
                    max_page_lifetime_ms: self.config.default_timeout_ms.unwrap_or(30_000),
                },
                script_support: true,
            }),
            credential: None,
        })
    }
}

#[cfg(test)]
#[path = "chrome_render_tests.rs"]
mod tests;
