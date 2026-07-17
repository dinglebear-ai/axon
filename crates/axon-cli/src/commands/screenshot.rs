mod util;

#[cfg(test)]
mod screenshot_migration_tests;

use super::common::parse_urls;
use axon_core::config::Config;
use axon_core::http::{normalize_url, validate_url};
use axon_core::logging::log_done;
use axon_core::ui::{primary, print_option, print_phase};
use axon_services::screenshot::screenshot_capture;
use std::error::Error;
use util::require_chrome;

pub(crate) fn print_screenshot_preamble(cfg: &Config, normalized: &str) {
    print_phase("◐", "Screenshot", normalized);
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
}

pub(crate) fn emit_screenshot_result(
    cfg: &Config,
    normalized: &str,
    result: &axon_services::types::ScreenshotResult,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string(result)?);
        log_done(&format!(
            "command=screenshot url={normalized} artifact_id={} format=png",
            result.artifact_id.0
        ));
    } else {
        let explicit_output = cfg
            .output_path
            .as_ref()
            .map(|path| format!(" output={}", path.display()))
            .unwrap_or_default();
        log_done(&format!(
            "captured: artifact_id={} url={normalized} format=png{explicit_output}",
            result.artifact_id.0
        ));
    }
    Ok(())
}

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

    print_screenshot_preamble(cfg, &normalized);

    let result = screenshot_capture(cfg, &normalized).await?;
    emit_screenshot_result(cfg, &normalized, &result)?;

    Ok(())
}
