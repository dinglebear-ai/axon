mod util;

#[cfg(test)]
mod screenshot_migration_tests;

use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::http::{normalize_url, validate_url};
use crate::core::logging::log_done;
use crate::core::ui::{primary, print_option, print_phase};
use crate::services::screenshot::screenshot_capture;
use std::error::Error;
use util::require_chrome;

pub async fn run_screenshot(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err(
            anyhow::anyhow!("screenshot requires at least one URL (positional or --urls)").into(),
        );
    }
    for url in &urls {
        screenshot_one(cfg, url).await?;
    }
    Ok(())
}

async fn screenshot_one(cfg: &Config, url: &str) -> Result<(), Box<dyn Error>> {
    require_chrome(cfg)?;

    let normalized = normalize_url(url);
    validate_url(&normalized)?;

    print_phase("◐", "Screenshot", &normalized);
    println!("  {}", primary("Options:"));
    print_option("fullPage", &cfg.screenshot_full_page.to_string());
    print_option(
        "viewport",
        &format!("{}x{}", cfg.viewport_width, cfg.viewport_height),
    );
    print_option(
        "chromeRemoteUrl",
        cfg.chrome_remote_url.as_deref().unwrap_or("none"),
    );
    println!();

    let result = screenshot_capture(cfg, &normalized).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string(&result)?);
        log_done(&format!(
            "command=screenshot url={normalized} bytes={} format=png",
            result.size_bytes
        ));
    } else {
        log_done(&format!(
            "saved: {} ({} bytes) url={normalized} format=png",
            result.path, result.size_bytes
        ));
    }

    Ok(())
}
