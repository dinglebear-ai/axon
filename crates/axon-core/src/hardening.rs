//! Process hardening helpers shared across Axon binaries.
//!
//! These live in `axon-core` so long-lived entrypoints (`axon serve`,
//! `axon mcp`) can enforce them without depending on the legacy `axon-vector`
//! crate, which historically owned this call alongside the ask document cache.

use crate::config::Config;
use anyhow::{Context, Result};

/// Disable process core dumps when the ask full-document cache is enabled in a
/// long-lived process. The cache can hold retrieved source text in heap memory.
pub fn enforce_core_dump_disabled_for_ask_cache(cfg: &Config) -> Result<()> {
    if !cfg.ask_cache_enabled {
        return Ok(());
    }

    enforce_core_dump_disabled()
}

#[cfg(unix)]
fn enforce_core_dump_disabled() -> Result<()> {
    let status = std::process::Command::new("prlimit")
        .arg("--core=0:0")
        .arg("--pid")
        .arg(std::process::id().to_string())
        .status()
        .context("failed to run prlimit while enforcing RLIMIT_CORE=0")?;
    anyhow::ensure!(
        status.success(),
        "failed to enforce RLIMIT_CORE=0 with prlimit; status={status}"
    );
    Ok(())
}

#[cfg(not(unix))]
fn enforce_core_dump_disabled() -> Result<()> {
    anyhow::bail!(
        "ask full-document cache is enabled, but RLIMIT_CORE enforcement is unsupported on this platform"
    )
}

#[cfg(test)]
#[path = "hardening_tests.rs"]
mod tests;
