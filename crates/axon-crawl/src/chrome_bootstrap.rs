//! Chrome runtime bootstrap: CDP probe, WebSocket URL pre-resolution, and
//! initial render mode resolution.
//!
//! Shared by both CLI sync-crawl and the services crawl_sync layer.

use crate::engine::resolve_cdp_ws_url;
use axon_core::config::{Config, RenderMode};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ChromeBootstrapOutcome {
    pub remote_ready: bool,
    /// Pre-resolved CDP WebSocket URL (`ws://host:port/devtools/browser/UUID`).
    pub resolved_ws_url: Option<String>,
    pub warnings: Vec<String>,
}

pub fn chrome_runtime_requested(cfg: &Config) -> bool {
    !cfg.cache_http_only && matches!(cfg.render_mode, RenderMode::Chrome | RenderMode::AutoSwitch)
}

pub async fn bootstrap_chrome_runtime(cfg: &Config) -> ChromeBootstrapOutcome {
    let mut outcome = ChromeBootstrapOutcome {
        remote_ready: false,
        resolved_ws_url: None,
        warnings: Vec::new(),
    };

    if !chrome_runtime_requested(cfg) {
        return outcome;
    }
    let Some(remote_url) = cfg.chrome_remote_url.as_deref() else {
        outcome.warnings.push(
            "AXON_CHROME_REMOTE_URL is unset; using Spider local Chrome launcher".to_string(),
        );
        return outcome;
    };

    let bootstrap_timeout = Duration::from_millis(cfg.chrome_bootstrap_timeout_ms);
    for attempt in 0..=cfg.chrome_bootstrap_retries {
        let probe = tokio::time::timeout(bootstrap_timeout, resolve_cdp_ws_url(remote_url));
        if let Ok(Some(ws_url)) = probe.await {
            outcome.remote_ready = true;
            outcome.resolved_ws_url = Some(ws_url);
            return outcome;
        }
        if attempt < cfg.chrome_bootstrap_retries {
            tokio::time::sleep(Duration::from_millis(200 * (attempt as u64 + 1))).await;
        }
    }

    outcome
        .warnings
        .push("remote chrome probe failed; falling back to local Chrome launcher".to_string());

    outcome
}

pub fn resolve_initial_mode(cfg: &Config) -> RenderMode {
    if cfg.cache_http_only {
        return RenderMode::Http;
    }
    match cfg.render_mode {
        RenderMode::AutoSwitch => RenderMode::Http,
        m => m,
    }
}
