use axon_core::config::Config;
use std::error::Error;

// Re-export for tests only — canonical implementation lives in src/crawl/screenshot.rs.
// Services/MCP import from there directly; only test modules in this subtree use the re-export.
#[cfg(test)]
pub(crate) use axon_crawl::screenshot::url_to_screenshot_filename;

/// Validate that Chrome is configured before attempting a screenshot.
pub(super) fn require_chrome(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.chrome_remote_url.is_none() {
        return Err(anyhow::anyhow!(
            "screenshot requires Chrome — set AXON_CHROME_REMOTE_URL in the env layer"
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
#[path = "util_tests.rs"]
mod tests;
