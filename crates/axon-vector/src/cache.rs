//! In-process document-chunk cache.
//!
//! Wraps [`moka::future::Cache`] with single-flight (`try_get_with`) +
//! per-collection generation-counter invalidation. See `doc_cache.rs` and
//! `generation.rs` for details.
//!
//! Process-local: only useful in long-lived parents (`axon serve`,
//! `axon mcp`). CLI one-shots see zero hit rate by definition.

mod doc_cache;
mod generation;
#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

use anyhow::{Context, Result};
use axon_core::config::Config;

pub use doc_cache::{
    CACHE_TTL_HARD_CAP_SECS, DocCache, DocCacheConfig, DocCacheKey, DocCacheStats,
    doc_cache_for_config,
};
pub use generation::{bump_generation, current_generation};

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
