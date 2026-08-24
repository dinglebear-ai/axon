//! Chrome runtime bootstrap: CDP probe, WebSocket URL pre-resolution, and
//! initial render mode resolution.
//!
//! Shared by both CLI sync-crawl and the services crawl_sync layer.

use crate::web_engine::engine::{cdp_probe_skipped_in_docker, resolve_cdp_ws_url};
use axon_core::config::{Config, RenderMode};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ChromeBootstrapOutcome {
    pub remote_ready: bool,
    /// Pre-resolved CDP WebSocket URL (`ws://host:port/devtools/browser/UUID`).
    pub resolved_ws_url: Option<String>,
    /// The probe actually ran (non-Docker host with a configured remote) and
    /// exhausted its retries — the remote endpoint is unreachable. Distinct
    /// from `remote_ready == false` alone, which also covers "no remote
    /// configured" and "probe skipped inside Docker".
    pub remote_unreachable: bool,
    pub warnings: Vec<String>,
}

pub fn chrome_runtime_requested(cfg: &Config) -> bool {
    !cfg.cache_http_only && matches!(cfg.render_mode, RenderMode::Chrome | RenderMode::AutoSwitch)
}

pub async fn bootstrap_chrome_runtime(cfg: &Config) -> ChromeBootstrapOutcome {
    let mut outcome = ChromeBootstrapOutcome {
        remote_ready: false,
        resolved_ws_url: None,
        remote_unreachable: false,
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

    // A pre-resolved ws:// URL needs no liveness probe — honor it everywhere,
    // including inside Docker (matches resolve_cdp_ws_url's own shortcut).
    if remote_url.starts_with("ws://") || remote_url.starts_with("wss://") {
        outcome.remote_ready = true;
        outcome.resolved_ws_url = Some(remote_url.to_string());
        return outcome;
    }

    // Inside Docker the probe cannot run (the remote hostname resolves on the
    // bridge network, not from here); spider gets the discovery URL as-is.
    if cdp_probe_skipped_in_docker() {
        return outcome;
    }

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

    outcome.remote_unreachable = true;
    outcome.warnings.push(format!(
        "remote chrome at {remote_url} is unreachable after {} probe attempt(s); \
         falling back to local Chrome launcher",
        cfg.chrome_bootstrap_retries + 1
    ));

    outcome
}

/// Fold a bootstrap outcome back into the render `Config`.
///
/// On success the pre-resolved `ws://` URL replaces the discovery URL so the
/// per-render `/json/version` round-trip is skipped. On a confirmed-unreachable
/// remote the URL is **cleared** — leaving it set would make spider redial the
/// dead endpoint (~11 attempts) and then silently degrade to a browserless
/// HTTP crawl instead of launching a local Chrome (bead axon_rust-nkh6y).
/// A skipped probe (Docker, or no remote configured) leaves the config as-is.
pub fn apply_bootstrap_outcome(cfg: &mut Config, outcome: &ChromeBootstrapOutcome) {
    if let Some(ws_url) = &outcome.resolved_ws_url {
        cfg.chrome_remote_url = Some(ws_url.clone());
    } else if outcome.remote_unreachable {
        cfg.chrome_remote_url = None;
    }
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

#[cfg(test)]
#[path = "chrome_bootstrap_tests.rs"]
mod tests;
