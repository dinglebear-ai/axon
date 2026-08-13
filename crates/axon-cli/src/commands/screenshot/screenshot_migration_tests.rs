//! Migration contract tests for the screenshot command.
//!
//! These tests document the stable contracts that MUST survive the
//! CDP → Spider screenshot migration:
//!   1. Chrome requirement produces a clear error
//!   2. JSON output exposes an opaque artifact identifier, never a path
//!   3. Filename sanitization is deterministic
//!
//! All tests exercise pure functions — no Chrome, no network.

use super::util::require_chrome;
use super::{write_json_screenshot_preamble, write_json_screenshot_result};
use axon_api::source::{ArtifactId, Timestamp};
use axon_core::config::Config;
use axon_core::paths::url_to_screenshot_filename;
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
    let parsed = serde_json::to_value(axon_services::types::ScreenshotResult {
        artifact_id: ArtifactId::new("art_screenshot_123"),
        width: 1280,
        height: 720,
        captured_at: Timestamp("2026-07-16T00:00:00Z".to_string()),
        warnings: Vec::new(),
    })
    .expect("must produce valid JSON");

    let obj = parsed.as_object().expect("top-level must be an object");

    assert_eq!(obj.len(), 5);
    assert_eq!(obj["artifact_id"], "art_screenshot_123");
    assert!(obj.get("path").is_none());
    assert!(obj.get("relative_path").is_none());
    assert!(obj.get("display_path").is_none());
}

#[test]
fn screenshot_json_keeps_stdout_machine_readable_and_default_stderr_silent() {
    let cfg = Config {
        json_output: true,
        chrome_remote_url: Some("http://127.0.0.1:6000".to_string()),
        ..Config::default()
    };
    let result = axon_services::types::ScreenshotResult {
        artifact_id: ArtifactId::new("art_screenshot_streams"),
        width: 1280,
        height: 720,
        captured_at: Timestamp("2026-07-30T00:00:00Z".to_string()),
        warnings: Vec::new(),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_json_screenshot_preamble(&cfg, "https://example.com", &mut stderr).unwrap();
    write_json_screenshot_result(
        &cfg,
        "https://example.com",
        &result,
        &mut stdout,
        &mut stderr,
    )
    .unwrap();

    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap()["artifact_id"],
        "art_screenshot_streams"
    );
    assert!(!stdout.contains("capturing"));
    assert!(!stdout.contains("completed"));
    assert!(stderr.is_empty());
}

#[test]
fn screenshot_json_verbose_progress_stays_on_stderr() {
    let cfg = Config {
        json_output: true,
        verbosity: 1,
        chrome_remote_url: Some("http://127.0.0.1:6000".to_string()),
        ..Config::default()
    };
    let result = axon_services::types::ScreenshotResult {
        artifact_id: ArtifactId::new("art_screenshot_verbose"),
        width: 1280,
        height: 720,
        captured_at: Timestamp("2026-07-30T00:00:00Z".to_string()),
        warnings: Vec::new(),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_json_screenshot_preamble(&cfg, "https://example.com", &mut stderr).unwrap();
    write_json_screenshot_result(
        &cfg,
        "https://example.com",
        &result,
        &mut stdout,
        &mut stderr,
    )
    .unwrap();

    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stderr.contains("screenshot: capturing"));
    assert!(stderr.contains("screenshot: completed"));
    serde_json::from_slice::<serde_json::Value>(&stdout).unwrap();
}

#[test]
fn screenshot_json_quiet_suppresses_progress_only() {
    let cfg = Config {
        json_output: true,
        quiet: true,
        chrome_remote_url: Some("http://127.0.0.1:6000".to_string()),
        ..Config::default()
    };
    let result = axon_services::types::ScreenshotResult {
        artifact_id: ArtifactId::new("art_screenshot_quiet"),
        width: 1280,
        height: 720,
        captured_at: Timestamp("2026-07-30T00:00:00Z".to_string()),
        warnings: Vec::new(),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_json_screenshot_preamble(&cfg, "https://example.com", &mut stderr).unwrap();
    write_json_screenshot_result(
        &cfg,
        "https://example.com",
        &result,
        &mut stdout,
        &mut stderr,
    )
    .unwrap();

    assert!(stderr.is_empty());
    assert_eq!(stdout.iter().filter(|byte| **byte == b'\n').count(), 1);
    serde_json::from_slice::<serde_json::Value>(&stdout).unwrap();
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
