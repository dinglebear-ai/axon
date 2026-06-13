//! CLI Chrome runtime shim — delegates to `src/crawl/chrome_bootstrap`.
//!
//! These functions are only used by `runtime_migration_tests.rs` now that
//! sync_crawl delegates to the services layer directly.

#[cfg(test)]
use crate::core::config::{Config, RenderMode};
#[cfg(test)]
pub use crate::crawl::chrome_bootstrap::ChromeBootstrapOutcome;

#[cfg(test)]
pub(super) fn chrome_runtime_requested(cfg: &Config) -> bool {
    crate::crawl::chrome_bootstrap::chrome_runtime_requested(cfg)
}

#[cfg(test)]
pub(super) async fn bootstrap_chrome_runtime(cfg: &Config) -> ChromeBootstrapOutcome {
    crate::crawl::chrome_bootstrap::bootstrap_chrome_runtime(cfg).await
}

#[cfg(test)]
pub(super) fn resolve_initial_mode(cfg: &Config) -> RenderMode {
    crate::crawl::chrome_bootstrap::resolve_initial_mode(cfg)
}
