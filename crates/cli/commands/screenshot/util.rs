use crate::crates::core::config::Config;
use std::error::Error;

// Re-export for tests only — canonical implementation lives in crates/crawl/screenshot.rs.
// Services/MCP import from there directly; only test modules in this subtree use the re-export.
#[cfg(test)]
pub(crate) use crate::crates::crawl::screenshot::url_to_screenshot_filename;

/// Validate that Chrome is configured before attempting a screenshot.
pub(super) fn require_chrome(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.chrome_remote_url.is_none() {
        return Err(anyhow::anyhow!(
            "screenshot requires Chrome — set AXON_CHROME_REMOTE_URL or pass --chrome-remote-url"
        )
        .into());
    }
    Ok(())
}

/// Format screenshot result as JSON for `--json` mode.
#[cfg(test)]
pub(super) fn format_screenshot_json(url: &str, path: &str, size_bytes: u64) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "url": url,
        "path": path,
        "size_bytes": size_bytes,
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;

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

    // --- format_screenshot_json ---

    #[test]
    fn test_json_output_format() {
        let json = format_screenshot_json("https://example.com", "/tmp/out.png", 12345);
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("output should be valid JSON");
        assert_eq!(parsed["url"], "https://example.com");
        assert_eq!(parsed["path"], "/tmp/out.png");
        assert_eq!(parsed["size_bytes"], 12345);
    }
}
