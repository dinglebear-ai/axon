//! Contract tests for the Chrome bootstrap runtime in `crawl/runtime.rs`.
//!
//! These tests validate the public API surface of the Chrome bootstrap module:
//! - `resolve_initial_mode()` — pure function, always passes
//! - `chrome_runtime_requested()` — pure function, always passes
//! - `bootstrap_chrome_runtime()` — async, exercises early-exit paths
//!   (no live Chrome instance required)
//!
//! Tests that depend on a live CDP endpoint or the shared engine resolver
//! (Task 2) are marked with comments explaining the expected behavior once
//! the migration is complete.

use super::runtime::{bootstrap_chrome_runtime, chrome_runtime_requested, resolve_initial_mode};
use crate::core::config::{Config, RenderMode};

// ---------------------------------------------------------------------------
// resolve_initial_mode — pure function tests
// ---------------------------------------------------------------------------

#[test]
fn resolve_initial_mode_autoswitch_starts_http() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        cache_http_only: false,
        ..Config::default()
    };
    assert_eq!(
        resolve_initial_mode(&cfg),
        RenderMode::Http,
        "AutoSwitch must resolve to Http for the initial crawl phase"
    );
}

#[test]
fn resolve_initial_mode_chrome_stays_chrome() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: false,
        ..Config::default()
    };
    assert_eq!(
        resolve_initial_mode(&cfg),
        RenderMode::Chrome,
        "explicit Chrome mode must pass through unchanged"
    );
}

#[test]
fn resolve_initial_mode_http_stays_http() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        cache_http_only: false,
        ..Config::default()
    };
    assert_eq!(
        resolve_initial_mode(&cfg),
        RenderMode::Http,
        "explicit Http mode must pass through unchanged"
    );
}

#[test]
fn resolve_initial_mode_cache_http_only_forces_http() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: true,
        ..Config::default()
    };
    assert_eq!(
        resolve_initial_mode(&cfg),
        RenderMode::Http,
        "cache_http_only=true must force Http regardless of render_mode"
    );
}

#[test]
fn resolve_initial_mode_cache_http_only_overrides_autoswitch() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        cache_http_only: true,
        ..Config::default()
    };
    assert_eq!(
        resolve_initial_mode(&cfg),
        RenderMode::Http,
        "cache_http_only=true must force Http even for AutoSwitch"
    );
}

// ---------------------------------------------------------------------------
// chrome_runtime_requested — pure function tests
// ---------------------------------------------------------------------------

#[test]
fn chrome_runtime_requested_true_for_chrome_mode() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: false,
        ..Config::default()
    };
    assert!(
        chrome_runtime_requested(&cfg),
        "Chrome mode with cache_http_only=false must request runtime"
    );
}

#[test]
fn chrome_runtime_requested_true_for_autoswitch_mode() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        cache_http_only: false,
        ..Config::default()
    };
    assert!(
        chrome_runtime_requested(&cfg),
        "AutoSwitch mode with cache_http_only=false must request runtime"
    );
}

#[test]
fn chrome_runtime_requested_false_for_http_mode() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        cache_http_only: false,
        ..Config::default()
    };
    assert!(
        !chrome_runtime_requested(&cfg),
        "Http mode must never request Chrome runtime"
    );
}

#[test]
fn chrome_runtime_requested_false_when_cache_http_only() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: true,
        ..Config::default()
    };
    assert!(
        !chrome_runtime_requested(&cfg),
        "cache_http_only=true must suppress Chrome runtime request"
    );
}

#[test]
fn chrome_runtime_requested_false_when_cache_http_only_autoswitch() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        cache_http_only: true,
        ..Config::default()
    };
    assert!(
        !chrome_runtime_requested(&cfg),
        "cache_http_only=true must suppress Chrome runtime even for AutoSwitch"
    );
}

// ---------------------------------------------------------------------------
// bootstrap_chrome_runtime — async tests (early-exit paths, no live Chrome)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bootstrap_skips_when_http_mode() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        cache_http_only: false,
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(!outcome.remote_ready, "Http mode must not probe Chrome");
    assert!(outcome.resolved_ws_url.is_none());
    assert!(
        outcome.warnings.is_empty(),
        "Http mode early-exit must produce no warnings"
    );
}

#[tokio::test]
async fn bootstrap_skips_when_cache_http_only() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: true,
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(!outcome.remote_ready, "cache_http_only must skip bootstrap");
    assert!(outcome.resolved_ws_url.is_none());
    assert!(
        outcome.warnings.is_empty(),
        "cache_http_only early-exit must produce no warnings"
    );
}

#[tokio::test]
async fn bootstrap_warns_when_no_remote_url() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: false,
        chrome_remote_url: None,
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(
        !outcome.remote_ready,
        "missing remote_url must not mark ready"
    );
    assert!(outcome.resolved_ws_url.is_none());
    assert_eq!(
        outcome.warnings.len(),
        1,
        "missing remote_url must produce exactly one warning"
    );
    assert!(
        outcome.warnings[0].contains("AXON_CHROME_REMOTE_URL is unset"),
        "warning must mention missing AXON_CHROME_REMOTE_URL, got: {}",
        outcome.warnings[0]
    );
}

#[tokio::test]
async fn bootstrap_warns_when_remote_url_unparseable() {
    // With the shared engine resolver, invalid schemes are not caught by a
    // separate parse step — they flow through resolve_cdp_ws_url which returns
    // None, causing the retry loop to exhaust and produce "probe failed".
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: false,
        chrome_bootstrap_retries: 0,
        chrome_remote_url: Some("ftp://invalid-scheme:9222".to_string()),
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(
        !outcome.remote_ready,
        "unparseable remote_url must not mark ready"
    );
    assert!(outcome.resolved_ws_url.is_none());
    assert_eq!(
        outcome.warnings.len(),
        1,
        "unparseable remote_url must produce exactly one warning"
    );
    assert!(
        outcome.warnings[0].contains("probe failed"),
        "warning must mention probe failure, got: {}",
        outcome.warnings[0]
    );
}

#[tokio::test]
async fn bootstrap_returns_warning_when_resolution_fails() {
    // Point at a port that nothing is listening on — probe must fail fast.
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        cache_http_only: false,
        chrome_bootstrap_timeout_ms: 250,
        chrome_bootstrap_retries: 0,
        chrome_remote_url: Some("http://127.0.0.1:1".to_string()),
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(
        !outcome.remote_ready,
        "unreachable endpoint must not mark ready"
    );
    assert!(
        outcome.resolved_ws_url.is_none(),
        "unreachable endpoint must not produce a WS URL"
    );
    assert!(
        !outcome.warnings.is_empty(),
        "unreachable endpoint must produce at least one warning"
    );
    assert!(
        outcome.warnings.iter().any(|w| w.contains("probe failed")),
        "warnings must mention probe failure, got: {:?}",
        outcome.warnings
    );
}

// ---------------------------------------------------------------------------
// ChromeBootstrapOutcome struct contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bootstrap_outcome_default_state_is_not_ready() {
    // Verify the initial state of the outcome struct when bootstrap is a no-op.
    let cfg = Config {
        render_mode: RenderMode::Http,
        ..Config::default()
    };
    let outcome = bootstrap_chrome_runtime(&cfg).await;
    assert!(!outcome.remote_ready);
    assert!(outcome.resolved_ws_url.is_none());
    assert!(outcome.warnings.is_empty());
}
