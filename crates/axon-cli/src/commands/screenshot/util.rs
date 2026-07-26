use axon_core::config::Config;
use std::error::Error;

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

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
