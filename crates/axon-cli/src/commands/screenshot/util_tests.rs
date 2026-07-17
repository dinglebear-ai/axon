use super::*;
use axon_core::config::Config;

// --- url_to_screenshot_filename ---

#[test]
fn test_url_to_screenshot_filename_basic() {
    let name = url_to_screenshot_filename("https://example.com/docs/intro", 1);
    assert_eq!(name, "0001-example-com-docs-intro.png");
}

#[test]
fn test_url_to_screenshot_filename_special_chars() {
    let name = url_to_screenshot_filename("https://foo.bar/a?b=c&d=e", 3);
    assert!(name.starts_with("0003-"));
    assert!(name.ends_with(".png"));
    // Should not contain raw special chars.
    assert!(!name.contains('?'));
    assert!(!name.contains('&'));
    assert!(!name.contains('='));
}

#[test]
fn test_url_to_screenshot_filename_long_url() {
    let long = format!("https://example.com/{}", "a".repeat(200));
    let name = url_to_screenshot_filename(&long, 1);
    assert!(name.ends_with(".png"));
    // The stem (before .png) should be truncated.
    assert!(name.len() < 200, "filename should be truncated: {name}");
}

#[test]
fn test_url_to_screenshot_filename_no_consecutive_hyphens() {
    let name = url_to_screenshot_filename("https://example.com/a///b..c", 1);
    assert!(!name.contains("--"), "should not have consecutive hyphens");
}

// --- require_chrome ---

#[test]
fn test_require_chrome_errors_when_missing() {
    let cfg = Config {
        chrome_remote_url: None,
        ..Config::default()
    };
    let result = require_chrome(&cfg);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("requires Chrome"),
        "error should mention Chrome requirement: {msg}"
    );
}

#[test]
fn test_require_chrome_ok_when_set() {
    let cfg = Config {
        chrome_remote_url: Some("ws://localhost:9222".to_string()),
        ..Config::default()
    };
    assert!(require_chrome(&cfg).is_ok());
}
