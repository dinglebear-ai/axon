//! Shared Spider Chrome configuration for crawl, scrape, map rendering, and refetch.

use axon_core::config::{Config, RenderMode};
use axon_core::http::{cdp_discovery_url, ssrf_blacklist_compact_strings};
use axon_core::logging::log_warn;
use spider::features::chrome_common::{
    RequestInterceptConfiguration, ScreenShotConfig, ScreenshotParams, WaitForSelector,
};
use spider::website::Website;
use std::error::Error;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserTimeoutPolicy {
    FloorForBrowserWork,
    Exact,
}

pub(crate) fn chrome_intercept_config(cfg: &Config) -> RequestInterceptConfiguration {
    let mut intercept = RequestInterceptConfiguration::new(true);
    intercept.set_blacklist_patterns(Some(
        ssrf_blacklist_compact_strings()
            .iter()
            .map(ToString::to_string)
            .collect(),
    ));
    if cfg.chrome_remote_local_policy {
        intercept.set_remote_local_policy(true);
    }
    intercept
}

pub(crate) fn apply_spider_browser_defaults_with_timeout(
    cfg: &Config,
    website: &mut Website,
    mode: RenderMode,
    timeout_policy: BrowserTimeoutPolicy,
) {
    if !matches!(mode, RenderMode::Chrome) {
        return;
    }
    website
        .with_chrome_intercept(chrome_intercept_config(cfg))
        .with_stealth(true)
        .with_fingerprint(true)
        .with_dismiss_dialogs(true);
    website.configuration.disable_log = true;
    if cfg.bypass_csp {
        website.with_csp_bypass(true);
    }
    website.with_wait_for_idle_network0(Some(spider::configuration::WaitForIdleNetwork::new(
        Some(Duration::from_secs(cfg.chrome_network_idle_timeout_secs)),
    )));
    let default_timeout_ms = (cfg.chrome_network_idle_timeout_secs + 30) * 1_000;
    let timeout_ms = match (cfg.request_timeout_ms, timeout_policy) {
        (Some(timeout_ms), BrowserTimeoutPolicy::Exact) => timeout_ms,
        (Some(timeout_ms), BrowserTimeoutPolicy::FloorForBrowserWork) => {
            timeout_ms.max(default_timeout_ms)
        }
        (None, _) => default_timeout_ms,
    };
    website.with_request_timeout(Some(Duration::from_millis(timeout_ms)));
    if let Some(selector) = &cfg.chrome_wait_for_selector {
        website.with_wait_for_selector(Some(WaitForSelector::new(
            Some(Duration::from_secs(cfg.chrome_network_idle_timeout_secs)),
            selector.clone(),
        )));
    }
    website.with_screenshot(Some(if cfg.chrome_screenshot {
        ScreenShotConfig::new(
            ScreenshotParams::default(),
            false,
            true,
            Some(cfg.output_dir.clone()),
        )
    } else {
        ScreenShotConfig::new(ScreenshotParams::default(), false, false, None)
    }));
}

pub(crate) async fn configure_spider_browser(
    cfg: &Config,
    mut website: Website,
    mode: RenderMode,
    timeout_policy: BrowserTimeoutPolicy,
) -> Result<Website, Box<dyn Error>> {
    if let Some(remote_url) = &cfg.chrome_remote_url {
        match super::engine::resolve_cdp_ws_url(remote_url).await {
            Some(ws_url) => {
                website.with_chrome_connection(Some(ws_url));
            }
            None if super::engine::cdp_probe_skipped_in_docker() => {
                // Inside Docker the hostname resolves on the bridge network;
                // hand spider the discovery URL unresolved.
                website.with_chrome_connection(Some(
                    cdp_discovery_url(remote_url).unwrap_or_else(|| remote_url.clone()),
                ));
            }
            None => {
                // Probe ran on a host and failed: leaving the dead endpoint
                // configured would make spider redial it (~11 attempts) and
                // then degrade to a browserless HTTP crawl. Skipping the
                // connection lets spider launch a local Chrome instead
                // (bead axon_rust-nkh6y).
                log_warn(&format!(
                    "remote chrome at {remote_url} is unreachable; \
                     using local Chrome launcher for this render"
                ));
            }
        }
    }
    apply_spider_browser_defaults_with_timeout(cfg, &mut website, mode, timeout_policy);
    if matches!(mode, RenderMode::Chrome) {
        website = website
            .build()
            .map_err(|error| format!("failed to build website with chrome settings: {error}"))?;
    }
    Ok(website)
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
