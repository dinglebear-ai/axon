//! Migration contract tests for the screenshot command.
//!
//! These tests document the stable contracts that MUST survive the
//! CDP → Spider screenshot migration:
//!   1. Chrome requirement produces a clear error
//!   2. JSON output has exactly {url, path, size_bytes}
//!   3. Filename sanitization is deterministic
//!
//! All tests exercise pure functions — no Chrome, no network.

use super::util::{format_screenshot_json, require_chrome, url_to_screenshot_filename};
use crate::core::config::Config;
use spider::features::chrome_common::{ScreenShotConfig, ScreenshotParams};

// ── 1. Chrome requirement ───────────────────────────────────────────

#[test]
fn screenshot_requires_chrome_remote_url() {
    let cfg = Config {
        chrome_remote_url: None,
        ..Config::default()
    };
    let err = require_chrome(&cfg).unwrap_err();
    let msg = err.to_string();

    assert!(
        msg.contains("Chrome"),
        "error must mention Chrome so the user knows what to configure: {msg}"
    );
    assert!(
        msg.contains("AXON_CHROME_REMOTE_URL"),
        "error should reference the env var: {msg}"
    );
}

// ── 2. JSON output contract ─────────────────────────────────────────

#[test]
fn screenshot_json_contract_is_stable() {
    let json_str = format_screenshot_json("https://example.com/page", "/tmp/shot.png", 42000);
    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).expect("must produce valid JSON");

    let obj = parsed.as_object().expect("top-level must be an object");

    // Exactly three keys — no more, no less.
    assert_eq!(
        obj.len(),
        3,
        "JSON contract must have exactly 3 keys, got: {:?}",
        obj.keys().collect::<Vec<_>>()
    );

    // Required keys with correct types.
    assert!(obj["url"].is_string(), "url must be a string");
    assert!(obj["path"].is_string(), "path must be a string");
    assert!(obj["size_bytes"].is_number(), "size_bytes must be a number");

    // Values pass through unchanged.
    assert_eq!(obj["url"], "https://example.com/page");
    assert_eq!(obj["path"], "/tmp/shot.png");
    assert_eq!(obj["size_bytes"], 42000);
}

// ── 3. Filename sanitization ────────────────────────────────────────

#[test]
fn screenshot_filename_sanitizes_url() {
    // Basic URL → deterministic filename.
    let name = url_to_screenshot_filename("https://docs.rs/axon/latest/guide", 1);
    assert_eq!(name, "0001-docs-rs-axon-latest-guide.png");

    // Same input, same output (deterministic).
    let again = url_to_screenshot_filename("https://docs.rs/axon/latest/guide", 1);
    assert_eq!(name, again, "filename generation must be deterministic");

    // Index zero-pads to 4 digits.
    let name_99 = url_to_screenshot_filename("https://example.com", 99);
    assert!(
        name_99.starts_with("0099-"),
        "index must be zero-padded to 4 digits: {name_99}"
    );

    // All filenames end with .png.
    assert!(name.ends_with(".png"));

    // No unsafe filesystem characters survive.
    let nasty = url_to_screenshot_filename("https://evil.com/<script>?q=a&b=c#frag", 2);
    for bad in &['/', '<', '>', '?', '&', '#', '='] {
        assert!(
            !nasty.contains(*bad),
            "filename must not contain '{bad}': {nasty}"
        );
    }
}

// ── 4. Full-page flag propagation ─────────────────────────────────

#[test]
fn screenshot_full_page_flag_is_honored() {
    // Verify that ScreenShotConfig correctly carries full_page=true vs false.
    // This is the Spider config path used by spider_capture.rs.

    let params_full = ScreenshotParams {
        full_page: Some(true),
        ..Default::default()
    };
    let config_full = ScreenShotConfig::new(params_full, true, false, None);
    assert_eq!(config_full.params.full_page, Some(true));
    assert!(
        config_full.bytes,
        "bytes must be true for in-memory capture"
    );
    assert!(!config_full.save, "save must be false — we handle writing");

    let params_viewport = ScreenshotParams {
        full_page: Some(false),
        ..Default::default()
    };
    let config_viewport = ScreenShotConfig::new(params_viewport, true, false, None);
    assert_eq!(config_viewport.params.full_page, Some(false));

    // Default params should have full_page=None (unset).
    let params_default = ScreenshotParams::default();
    assert_eq!(params_default.full_page, None);
}
